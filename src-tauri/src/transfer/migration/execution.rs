use std::sync::atomic::{AtomicU8, Ordering};

use crate::{
    bucket::{
        model::{Configuration, ConfigurationChange},
        repository as buckets,
        service::BucketService,
    },
    common::error::{AppError, Result},
    connection::service::Session,
    object::service::ObjectService,
    transfer::sts,
};

use super::{
    execution_model::{BucketStatus, CleanupProof, Ownership, Stage, Status},
    history,
    model::{Effect, Mapping, Migration, Side},
    ownership,
    preflight::{
        PolicyAccess, feature_name, inspect_source, protect, require_absent, restore_effect,
        verify_policy, verify_sample_read, worker_principal,
    },
    replay::{self, Evidence, Replay},
    repository::MigrationRepository,
};

#[derive(Default)]
pub struct Signal(AtomicU8);

impl Signal {
    pub fn pause(&self) {
        self.0.fetch_max(1, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.0.store(2, Ordering::Relaxed);
    }

    fn running(&self) -> bool {
        self.0.load(Ordering::Relaxed) == 0
    }

    fn stopped(&self) -> bool {
        self.0.load(Ordering::Relaxed) == 2
    }
}

pub struct Execution<'a> {
    pub repository: &'a MigrationRepository,
    pub source: &'a Session,
    pub target: &'a Session,
}

impl Execution<'_> {
    pub async fn run(&self, record: &mut Migration, signal: &Signal) -> Result<()> {
        let stage = record.run.as_ref().unwrap().stage;
        let count = record.run.as_ref().unwrap().buckets.len();
        for index in 0..count {
            if !signal.running() {
                break;
            }
            let bucket = &record.run.as_ref().unwrap().buckets[index];
            if matches!(
                bucket.status,
                BucketStatus::Verified | BucketStatus::Removed | BucketStatus::Failed
            ) || (stage == Stage::PreCopy && bucket.status == BucketStatus::AwaitingCutover)
            {
                continue;
            }
            let mapping = bucket.mapping.clone();
            if let Err(error) = self
                .copy_bucket(record, index, &mapping, stage, signal)
                .await
            {
                let bucket = &mut record.run.as_mut().unwrap().buckets[index];
                bucket.status = BucketStatus::Failed;
                bucket.error = Some(error.to_string());
                bucket.current_key = None;
                self.repository.save(record)?;
                self.restore(record, |effect| belongs_to(effect, &mapping))
                    .await?;
            }
        }
        if signal.stopped() {
            self.invalidate_cleanup(record);
            self.restore(record, |_| true).await?;
            record.run.as_mut().unwrap().status = Status::Stopped;
        } else if !signal.running() {
            record.run.as_mut().unwrap().status = Status::Paused;
        } else {
            record.run.as_mut().unwrap().finish();
        }
        self.repository.save(record)
    }

    async fn copy_bucket(
        &self,
        record: &mut Migration,
        index: usize,
        mapping: &Mapping,
        stage: Stage,
        signal: &Signal,
    ) -> Result<()> {
        let draft = record.draft.clone();
        let bucket = &mut record.run.as_mut().unwrap().buckets[index];
        bucket.status = BucketStatus::Preparing;
        bucket.error = None;
        self.repository.save(record)?;
        let check = inspect_source(self.source, &draft, mapping).await;
        if !check.plan.issues.is_empty() {
            return Err(AppError::Invalid(check.plan.issues.join(" ")));
        }
        let report = record
            .report
            .as_ref()
            .and_then(|report| report.pairs.iter().find(|plan| &plan.mapping == mapping));
        if !report.is_some_and(|plan| plan.settings_copied == check.plan.settings_copied) {
            return Err(AppError::Invalid(
                "Source features changed since the reviewed copy plan.".into(),
            ));
        }
        let principal = worker_principal(&draft, self.target).await?;
        let freeze = stage == Stage::Cutover;
        if freeze || draft.grant_source_read {
            protect(
                self.repository,
                record,
                self.source,
                Side::Source,
                &mapping.source,
                PolicyAccess {
                    delete_only: false,
                    deny_writes: freeze,
                    writer: None,
                    read_grant: principal.as_deref().filter(|_| draft.grant_source_read),
                },
            )
            .await?;
        }
        if let Some(sample) = &check.sample {
            verify_sample_read(self.source, sample).await?;
        }
        if !signal.running() {
            return Ok(());
        }
        ownership::prepare(self.repository, record, self.target, index).await?;
        if draft.protect_destination {
            protect(
                self.repository,
                record,
                self.target,
                Side::Target,
                &mapping.target,
                PolicyAccess {
                    delete_only: false,
                    deny_writes: true,
                    writer: principal.as_deref(),
                    read_grant: None,
                },
            )
            .await?;
        }
        if draft.history {
            BucketService::finish_versioning(self.target, &mapping.target, None).await?;
        }
        let bucket = &mut record.run.as_mut().unwrap().buckets[index];
        let baseline = bucket.baseline.clone();
        bucket.status = BucketStatus::Copying;
        self.repository.save(record)?;
        let replay = Replay {
            source: self.source,
            target: self.target,
            mapping,
            history: draft.history,
            method: draft.method,
            parallelism: draft.parallelism,
        };
        let copied = replay::sync(replay, &baseline, |completed, total, key| {
            let bucket = &mut record.run.as_mut().unwrap().buckets[index];
            bucket.completed = completed;
            bucket.total = total;
            bucket.current_key = key.map(str::to_owned);
            self.repository.save(record)?;
            Ok(signal.running())
        })
        .await?;
        if !copied || !signal.running() {
            return Ok(());
        }
        record.run.as_mut().unwrap().buckets[index].status = BucketStatus::Verifying;
        self.repository.save(record)?;
        let target = self
            .evidence(
                self.target,
                &mapping.target,
                draft.history,
                draft.parallelism,
            )
            .await?;
        if freeze {
            self.require_protection(record, Side::Source, &mapping.source, self.source)
                .await?;
            let source = self
                .evidence(
                    self.source,
                    &mapping.source,
                    draft.history,
                    draft.parallelism,
                )
                .await?;
            if source != target {
                return Err(AppError::Storage(
                    "Verification found different keys, content, tags or ordered history.".into(),
                ));
            }
            self.apply_settings(record, index, &check.settings).await?;
            self.restore(record, |effect| {
                matches!(effect,
                Effect::Policy { side: Side::Target, bucket, .. } if bucket == &mapping.target)
            })
            .await?;
        } else {
            self.restore(record, |effect| {
                matches!(effect,
                Effect::Policy { side: Side::Source, bucket, .. } if bucket == &mapping.source)
            })
            .await?;
        }
        let bucket = &mut record.run.as_mut().unwrap().buckets[index];
        bucket.baseline = target;
        bucket.status = if freeze {
            BucketStatus::Verified
        } else {
            BucketStatus::AwaitingCutover
        };
        self.repository.save(record)
    }

    async fn apply_settings(
        &self,
        record: &Migration,
        index: usize,
        settings: &[(Configuration, String)],
    ) -> Result<()> {
        let bucket = &record.run.as_ref().unwrap().buckets[index];
        for (feature, document) in settings {
            if *feature == Configuration::Tagging {
                ownership::copy_tags(
                    self.source,
                    self.target,
                    &bucket.mapping.source,
                    &bucket.mapping.target,
                    token(bucket.ownership)?,
                )
                .await?;
                continue;
            }
            buckets::configure(
                self.target,
                &bucket.mapping.target,
                *feature,
                ConfigurationChange::Write(document.clone()),
            )
            .await
            .map_err(|_| {
                AppError::Storage(format!(
                    "Could not copy {} to the destination.",
                    feature_name(*feature)
                ))
            })?;
            let observed = buckets::configure(
                self.target,
                &bucket.mapping.target,
                *feature,
                ConfigurationChange::Read,
            )
            .await
            .map_err(|_| {
                AppError::Storage(format!(
                    "Could not read back destination {}.",
                    feature_name(*feature)
                ))
            })?;
            if observed != *document {
                return Err(AppError::Storage(format!(
                    "The provider did not verify destination {}.",
                    feature_name(*feature)
                )));
            }
        }
        Ok(())
    }

    async fn evidence(
        &self,
        session: &Session,
        bucket: &str,
        history: bool,
        parallelism: usize,
    ) -> Result<Evidence> {
        let inventory = history::inventory(session, bucket, history).await?;
        replay::fingerprints(&session.s3, &inventory, parallelism).await
    }

    pub async fn restore(
        &self,
        record: &mut Migration,
        selected: impl Fn(&Effect) -> bool,
    ) -> Result<bool> {
        let mut errors = Vec::new();
        for index in (0..record.private.effects.len()).rev() {
            let effect = &record.private.effects[index];
            if !selected(effect)
                || blocked_by_later_effect(effect, &record.private.effects[index + 1..])
            {
                continue;
            }
            match restore_effect(effect, self.source, self.target).await {
                Ok(()) => {
                    record.private.effects.remove(index);
                    self.repository.save(record)?;
                }
                Err(error) => errors.push(format!("{} · {error}", effect_bucket(effect))),
            }
        }
        if !errors.is_empty() {
            record.recovery_error = Some(errors.join(" "));
        } else if record.private.effects.is_empty() {
            record.recovery_error = None;
        }
        self.repository.save(record)?;
        Ok(errors.is_empty())
    }

    pub fn invalidate_cleanup(&self, record: &mut Migration) {
        if let Some(run) = &mut record.run {
            for bucket in &mut run.buckets {
                bucket.cleanup = None;
            }
        }
    }

    pub async fn remove_incomplete(&self, record: &mut Migration, indices: &[usize]) -> Result<()> {
        for &index in indices {
            let result = self.remove_bucket(record, index).await;
            let bucket = &mut record.run.as_mut().unwrap().buckets[index];
            match result {
                Ok(()) => {
                    bucket.status = BucketStatus::Removed;
                    bucket.cleanup_error = None;
                }
                Err(error) => bucket.cleanup_error = Some(error.to_string()),
            }
            self.repository.save(record)?;
        }
        Ok(())
    }

    async fn remove_bucket(&self, record: &mut Migration, index: usize) -> Result<()> {
        let bucket = &record.run.as_ref().unwrap().buckets[index];
        let target = bucket.mapping.target.clone();
        let token = token(bucket.ownership)?;
        if require_absent(&self.target.s3, &target).await.is_ok() {
            discard_receipts(record, Side::Target, &target);
            return self.repository.save(record);
        }
        ownership::verify(self.target, &target, token).await?;
        let writer = sts::caller_principal(self.target).await?;
        protect(
            self.repository,
            record,
            self.target,
            Side::Target,
            &target,
            PolicyAccess {
                delete_only: false,
                deny_writes: true,
                writer: Some(&writer),
                read_grant: None,
            },
        )
        .await?;
        let result = self.delete_owned_target(record, &target, token).await;
        if result.is_err() {
            self.restore(record, |effect| matches!(effect, Effect::Policy { side: Side::Target, bucket, .. } if bucket == &target)).await?;
        }
        result
    }

    async fn delete_owned_target(
        &self,
        record: &mut Migration,
        target: &str,
        token: uuid::Uuid,
    ) -> Result<()> {
        let client = crate::transfer::security::write_client(&self.target.s3);
        for upload in ObjectService::multipart(&self.target.s3, target, "").await? {
            ObjectService::abort_multipart(&client, target, &upload.key, &upload.upload_id).await?;
        }
        let inventory = history::inventory(self.target, target, true).await?;
        for entries in inventory.values() {
            for entry in entries {
                replay::remove_entry(&self.target.s3, entry).await?;
            }
        }
        ownership::verify(self.target, target, token).await?;
        self.require_protection(record, Side::Target, target, self.target)
            .await?;
        BucketService::delete(self.target, target).await?;
        discard_receipts(record, Side::Target, target);
        self.repository.save(record)
    }

    pub async fn delete_sources(&self, record: &mut Migration, indices: &[usize]) -> Result<()> {
        for &index in indices {
            let result = self.delete_source(record, index).await;
            let bucket = &mut record.run.as_mut().unwrap().buckets[index];
            bucket.cleanup_error = result.err().map(|error| error.to_string());
            self.repository.save(record)?;
        }
        Ok(())
    }

    async fn delete_source(&self, record: &mut Migration, index: usize) -> Result<()> {
        let bucket = &record.run.as_ref().unwrap().buckets[index];
        let mapping = bucket.mapping.clone();
        let ownership = token(bucket.ownership)?;
        let prior = bucket.cleanup.clone();
        ownership::verify(self.target, &mapping.target, ownership).await?;
        if let Some(prior) = &prior
            && require_absent(&self.source.s3, &mapping.source)
                .await
                .is_ok()
        {
            let target = self
                .evidence(self.target, &mapping.target, true, record.draft.parallelism)
                .await?;
            if replay::manifest_digest(&target)? != prior.destination_digest {
                return Err(cleanup_mismatch());
            }
            self.finish_source_delete(record, index, &mapping.source)?;
            self.restore(record, |effect| {
                matches!(effect,
                Effect::Policy { side: Side::Target, bucket, .. } if bucket == &mapping.target)
            })
            .await?;
            return Ok(());
        }
        self.require_protection(record, Side::Source, &mapping.source, self.source)
            .await?;
        protect(
            self.repository,
            record,
            self.target,
            Side::Target,
            &mapping.target,
            PolicyAccess {
                delete_only: false,
                deny_writes: true,
                writer: None,
                read_grant: None,
            },
        )
        .await?;
        let result = self
            .delete_proven_source(record, index, &mapping, prior)
            .await;
        if result.is_ok()
            || record.run.as_ref().unwrap().buckets[index]
                .cleanup
                .is_none()
        {
            self.restore(record, |effect| matches!(effect, Effect::Policy { side: Side::Target, bucket, .. } if bucket == &mapping.target)).await?;
        }
        result
    }

    async fn delete_proven_source(
        &self,
        record: &mut Migration,
        index: usize,
        mapping: &Mapping,
        prior: Option<CleanupProof>,
    ) -> Result<()> {
        let parallelism = record.draft.parallelism;
        let target = self
            .evidence(self.target, &mapping.target, true, parallelism)
            .await?;
        let digest = replay::manifest_digest(&target)?;
        let inventory = history::inventory(self.source, &mapping.source, true).await?;
        let source = replay::fingerprints(&self.source.s3, &inventory, parallelism).await?;
        let versions = replay::bind_versions(&inventory, &source)?;
        if let Some(prior) = &prior {
            if digest != prior.destination_digest
                || !replay::remaining_verified(&versions, &prior.source_versions)
            {
                return Err(cleanup_mismatch());
            }
        } else if source != target {
            return Err(cleanup_mismatch());
        }
        let writer = sts::caller_principal(self.source).await?;
        protect(
            self.repository,
            record,
            self.source,
            Side::Source,
            &mapping.source,
            PolicyAccess {
                delete_only: true,
                deny_writes: true,
                writer: Some(&writer),
                read_grant: None,
            },
        )
        .await?;
        if prior.is_none() {
            record.run.as_mut().unwrap().buckets[index].cleanup = Some(CleanupProof {
                destination_digest: digest.clone(),
                source_versions: versions,
            });
            self.repository.save(record)?;
        }
        self.require_protection(record, Side::Source, &mapping.source, self.source)
            .await?;
        self.require_protection(record, Side::Target, &mapping.target, self.target)
            .await?;
        if replay::manifest_digest(
            &self
                .evidence(self.target, &mapping.target, true, parallelism)
                .await?,
        )? != digest
        {
            return Err(cleanup_mismatch());
        }
        for entries in inventory.values() {
            for entry in entries {
                replay::remove_entry(&self.source.s3, entry).await?;
            }
        }
        BucketService::delete(self.source, &mapping.source).await?;
        self.finish_source_delete(record, index, &mapping.source)
    }

    fn finish_source_delete(
        &self,
        record: &mut Migration,
        index: usize,
        source: &str,
    ) -> Result<()> {
        discard_receipts(record, Side::Source, source);
        let bucket = &mut record.run.as_mut().unwrap().buckets[index];
        bucket.source_deleted = true;
        bucket.cleanup = None;
        self.repository.save(record)
    }

    async fn require_protection(
        &self,
        record: &Migration,
        side: Side,
        name: &str,
        session: &Session,
    ) -> Result<()> {
        let expected = record
            .private
            .effects
            .iter()
            .rev()
            .find_map(|effect| match effect {
                Effect::Policy {
                    side: bound,
                    bucket,
                    protection,
                    ..
                } if *bound == side && name == bucket => Some(protection),
                _ => None,
            })
            .ok_or_else(|| AppError::Invalid("Recorded write protection is required.".into()))?;
        verify_policy(session, name, expected).await
    }
}

fn belongs_to(effect: &Effect, mapping: &Mapping) -> bool {
    match effect {
        Effect::Policy {
            side: Side::Source,
            bucket,
            ..
        } => bucket == &mapping.source,
        Effect::Policy {
            side: Side::Target,
            bucket,
            ..
        } => bucket == &mapping.target,
        Effect::Probe { .. } => false,
    }
}

fn blocked_by_later_effect(effect: &Effect, remaining: &[Effect]) -> bool {
    remaining.iter().any(|later| match (effect, later) {
        (
            Effect::Policy { side, bucket, .. },
            Effect::Policy {
                side: other,
                bucket: other_bucket,
                ..
            },
        ) => side == other && bucket == other_bucket,
        (
            Effect::Probe { bucket, .. },
            Effect::Policy {
                side: Side::Target,
                bucket: other,
                ..
            },
        ) => bucket == other,
        _ => false,
    })
}

fn effect_bucket(effect: &Effect) -> &str {
    match effect {
        Effect::Policy { bucket, .. } | Effect::Probe { bucket, .. } => bucket,
    }
}

fn discard_receipts(record: &mut Migration, side: Side, name: &str) {
    record.private.effects.retain(|effect| {
        !matches!(effect,
        Effect::Policy { side: bound, bucket, .. } if *bound == side && bucket == name)
    });
}

fn token(ownership: Ownership) -> Result<uuid::Uuid> {
    if let Ownership::Owned { token } = ownership {
        return Ok(token);
    }
    Err(AppError::Invalid(
        "This operation requires a durable destination ownership receipt.".into(),
    ))
}

fn cleanup_mismatch() -> AppError {
    AppError::Invalid(
        "Source cleanup refused because fresh content, tags or ordered versions differ.".into(),
    )
}
