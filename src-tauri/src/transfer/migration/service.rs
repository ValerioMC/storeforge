use std::path::PathBuf;

use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::{
        model::{IdentityKind, Workspace},
        service::{ConnectionService, Session},
    },
};

use super::{
    model::{Draft, Effect, Migration, MigrationView, Phase, Report, Side},
    preflight::{active_pair, read_pair, restore_effect},
    repository::MigrationRepository,
};

pub struct MigrationService {
    repository: MigrationRepository,
    operations: Mutex<()>,
}

impl MigrationService {
    pub fn open(directory: PathBuf) -> Result<Self> {
        Ok(Self {
            repository: MigrationRepository::open(directory)?,
            operations: Mutex::new(()),
        })
    }

    pub fn list(&self, source: Uuid) -> Result<Vec<MigrationView>> {
        Ok(self
            .repository
            .list(source)?
            .into_iter()
            .map(Migration::view)
            .collect())
    }

    pub fn save(
        &self,
        connections: &ConnectionService,
        draft: Draft,
        id: Option<Uuid>,
        revision: Option<Uuid>,
    ) -> Result<MigrationView> {
        let _operation = self.acquire()?;
        if let Some(id) = id {
            let saved = self.repository.get(id)?;
            require_revision(&saved, revision)?;
            saved.require_recovered()?;
        } else if revision.is_some() {
            return Err(stale_draft());
        }
        let (source, target) = sessions(connections, &draft)?;
        draft.validate(&source, &target)?;
        let mut record = Migration::new(id.unwrap_or_else(Uuid::new_v4), draft, &source, &target);
        self.repository.save(&mut record)?;
        Ok(record.view())
    }

    pub async fn check(
        &self,
        connections: &ConnectionService,
        id: Uuid,
        revision: Uuid,
        phase: Phase,
        approved: bool,
    ) -> Result<MigrationView> {
        let _operation = self.acquire()?;
        let mut record = self.repository.get(id)?;
        require_revision(&record, Some(revision))?;
        record.require_recovered()?;
        if phase == Phase::Active
            && (!approved
                || !record
                    .report
                    .as_ref()
                    .is_some_and(|report| report.passed(revision, Phase::Read)))
        {
            return Err(AppError::Invalid(
                "Pass fresh read-only checks and confirm the temporary access changes.".into(),
            ));
        }
        let (source, target) = sessions(connections, &record.draft)?;
        record.validate_bindings(&source, &target)?;
        let draft = record.draft.clone();
        record.checking = Some(phase);
        record.report = None;
        self.repository.save(&mut record)?;
        let mut plans = Vec::new();
        for mapping in &draft.mappings {
            let mut check = read_pair(&source, &target, &draft, mapping).await;
            if phase == Phase::Active && check.plan.issues.is_empty() {
                if let Err(error) =
                    active_pair(&self.repository, &mut record, &source, &target, &check).await
                {
                    check.plan.issues.push(error.to_string());
                }
                if !self.restore_all(&mut record, &source, &target).await? {
                    check
                        .plan
                        .issues
                        .push("Temporary access or resources still require recovery.".into());
                }
            }
            plans.push(check.plan);
            if !record.private.effects.is_empty() {
                break;
            }
        }
        record.report = Some(Report::new(revision, phase, plans));
        record.checking = None;
        self.repository.save(&mut record)?;
        Ok(record.view())
    }

    pub async fn recover(
        &self,
        connections: &ConnectionService,
        id: Uuid,
        revision: Uuid,
    ) -> Result<MigrationView> {
        let _operation = self.acquire()?;
        let mut record = self.repository.get(id)?;
        require_revision(&record, Some(revision))?;
        let (source, target) = sessions(connections, &record.draft)?;
        record.validate_bindings(&source, &target)?;
        self.restore_all(&mut record, &source, &target).await?;
        record.report = None;
        record.checking = None;
        self.repository.save(&mut record)?;
        Ok(record.view())
    }

    async fn restore_all(
        &self,
        record: &mut Migration,
        source: &Session,
        target: &Session,
    ) -> Result<bool> {
        record.recovery_error = None;
        let mut errors = Vec::new();
        for index in (0..record.private.effects.len()).rev() {
            let effect = &record.private.effects[index];
            if let Effect::Probe { bucket, .. } = effect
                && record.private.effects.iter().any(|effect| matches!(effect,
                    Effect::Policy { side: Side::Target, bucket: protected, .. } if protected == bucket))
            {
                continue;
            }
            match restore_effect(effect, source, target).await {
                Ok(()) => {
                    record.private.effects.remove(index);
                    self.repository.save(record)?;
                }
                Err(error) => errors.push(error.to_string()),
            }
        }
        if !errors.is_empty() {
            record.recovery_error = Some(errors.join(" "));
        }
        self.repository.save(record)?;
        Ok(errors.is_empty())
    }

    fn acquire(&self) -> Result<MutexGuard<'_, ()>> {
        self.operations
            .try_lock()
            .map_err(|_| AppError::Invalid("Another migration action is running.".into()))
    }
}

fn sessions(connections: &ConnectionService, draft: &Draft) -> Result<(Session, Session)> {
    let source = connections.session_for(draft.source, Workspace::Manager, IdentityKind::S3)?;
    let target = connections.session_for(draft.target, Workspace::Manager, IdentityKind::S3)?;
    Ok((source, target))
}

fn require_revision(record: &Migration, revision: Option<Uuid>) -> Result<()> {
    if revision != Some(record.revision) {
        return Err(stale_draft());
    }
    Ok(())
}

fn stale_draft() -> AppError {
    AppError::Invalid(
        "The migration draft changed. Reopen it and review the current revision.".into(),
    )
}
