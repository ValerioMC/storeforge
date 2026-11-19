use std::{
    path::PathBuf,
    sync::{Arc, Mutex as ControlLock},
};

use tokio::sync::{Mutex, MutexGuard, OwnedMutexGuard};
use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::{
        model::{IdentityKind, Workspace},
        service::{ConnectionService, Session},
    },
};

use super::{
    execution::{Execution, Signal},
    execution_model::{Action, BucketStatus, Ownership, Run, Stage, Status},
    model::{Draft, Migration, MigrationView, Phase, Report},
    preflight::{active_pair, read_pair},
    repository::MigrationRepository,
};

type ActiveMigration = (Uuid, Arc<Signal>);

#[derive(Clone)]
pub struct MigrationService {
    repository: Arc<MigrationRepository>,
    operations: Arc<Mutex<()>>,
    control: Arc<ControlLock<Option<ActiveMigration>>>,
}

impl MigrationService {
    pub fn open(directory: PathBuf) -> Result<Self> {
        Ok(Self {
            repository: Arc::new(MigrationRepository::open(directory)?),
            operations: Arc::new(Mutex::new(())),
            control: Arc::new(ControlLock::new(None)),
        })
    }

    pub fn list(&self, source: Uuid) -> Result<Vec<MigrationView>> {
        self.repository.list(source)
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
            require_draft(&saved)?;
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
        require_draft(&record)?;
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
        let execution = self.execution(&source, &target);
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
                if !execution.restore(&mut record, |_| true).await? {
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
        record.validate_recovery_scope(&source, &target)?;
        let execution = self.execution(&source, &target);
        execution.invalidate_cleanup(&mut record);
        self.repository.save(&mut record)?;
        execution.restore(&mut record, |_| true).await?;
        if record.run.is_none() {
            record.report = None;
        } else if let Some(run) = &mut record.run
            && matches!(run.status, Status::Paused | Status::AwaitingCutover)
        {
            run.status = Status::Stopped;
        }
        record.checking = None;
        self.repository.save(&mut record)?;
        Ok(record.view())
    }

    pub async fn action(
        &self,
        connections: &ConnectionService,
        id: Uuid,
        revision: Uuid,
        action: Action,
        confirmation: Option<String>,
    ) -> Result<Option<MigrationView>> {
        if matches!(action, Action::Pause | Action::Stop)
            && let Some(signal) = self.signal(id)?
        {
            let record = self.repository.get(id)?;
            require_revision(&record, Some(revision))?;
            if action == Action::Pause {
                signal.pause();
            } else {
                signal.stop();
            }
            return Ok(Some(record.view()));
        }

        let guard = Arc::clone(&self.operations)
            .try_lock_owned()
            .map_err(|_| busy())?;
        let mut record = self.repository.get(id)?;
        require_revision(&record, Some(revision))?;
        let phrase = confirmation.as_deref().unwrap_or_default();
        if action == Action::DeleteRecord {
            record.require_recovered()?;
            confirm(phrase, "DELETE RECORD")?;
            self.repository.remove(id)?;
            return Ok(None);
        }
        let (source, target) = sessions(connections, &record.draft)?;
        if matches!(
            action,
            Action::Stop | Action::RemoveIncomplete | Action::DeleteSources
        ) {
            record.validate_recovery_scope(&source, &target)?;
        } else {
            record.validate_bindings(&source, &target)?;
        }
        let execution = self.execution(&source, &target);
        match action {
            Action::Start => {
                let run = Run::start(&record)?;
                confirm(
                    phrase,
                    if run.stage == Stage::Cutover {
                        "CUTOVER"
                    } else {
                        "START COPY"
                    },
                )?;
                record.run = Some(run);
            }
            Action::Cutover | Action::Resume | Action::Retry => {
                if record.recovery_error.is_some() {
                    return Err(AppError::Invalid(
                        "Restore access before starting another copy pass.".into(),
                    ));
                }
                prepare_pass(&mut record, action, phrase)?;
            }
            Action::Stop => {
                let run = record
                    .run
                    .as_mut()
                    .filter(|run| {
                        matches!(
                            run.status,
                            Status::Paused | Status::AwaitingCutover | Status::Failed
                        )
                    })
                    .ok_or_else(action_unavailable)?;
                run.status = Status::Stopped;
                execution.invalidate_cleanup(&mut record);
                self.repository.save(&mut record)?;
                execution.restore(&mut record, |_| true).await?;
                return Ok(Some(record.view()));
            }
            Action::RemoveIncomplete | Action::DeleteSources => {
                if record.recovery_error.is_some() {
                    return Err(AppError::Invalid(
                        "Restore access before starting another cleanup.".into(),
                    ));
                }
                let indices = cleanup_targets(&record, action, phrase)?;
                if action == Action::RemoveIncomplete {
                    execution.remove_incomplete(&mut record, &indices).await?;
                } else {
                    execution.delete_sources(&mut record, &indices).await?;
                }
                return Ok(Some(record.view()));
            }
            Action::Pause | Action::DeleteRecord => return Err(action_unavailable()),
        }
        self.repository.save(&mut record)?;
        let view = record.view();
        self.spawn(record, source, target, guard)?;
        Ok(Some(view))
    }

    fn execution<'a>(&'a self, source: &'a Session, target: &'a Session) -> Execution<'a> {
        Execution {
            repository: &self.repository,
            source,
            target,
        }
    }

    fn signal(&self, id: Uuid) -> Result<Option<Arc<Signal>>> {
        let control = self.control.lock().map_err(|_| busy())?;
        Ok(control
            .as_ref()
            .filter(|(active, _)| *active == id)
            .map(|(_, signal)| Arc::clone(signal)))
    }

    fn spawn(
        &self,
        mut record: Migration,
        source: Session,
        target: Session,
        guard: OwnedMutexGuard<()>,
    ) -> Result<()> {
        let signal = Arc::new(Signal::default());
        *self.control.lock().map_err(|_| busy())? = Some((record.id, Arc::clone(&signal)));
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            let _guard = guard;
            if let Err(error) = service
                .execution(&source, &target)
                .run(&mut record, &signal)
                .await
            {
                tracing::error!(id = %record.id, %error, "Migration worker stopped");
                record.run.as_mut().unwrap().status = Status::Paused;
                record.recovery_error = Some(
                    "Migration progress could not be saved. Review access before continuing."
                        .into(),
                );
                if let Err(error) = service.repository.save(&mut record) {
                    tracing::error!(id = %record.id, %error, "Could not persist the migration checkpoint");
                }
            }
            match service.control.lock() {
                Ok(mut control) => *control = None,
                Err(error) => tracing::error!(%error, "Migration control could not be cleared"),
            }
        });
        Ok(())
    }

    fn acquire(&self) -> Result<MutexGuard<'_, ()>> {
        self.operations.try_lock().map_err(|_| busy())
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

fn require_draft(record: &Migration) -> Result<()> {
    if record.run.is_some() {
        return Err(AppError::Invalid(
            "The execution plan is locked after copy starts.".into(),
        ));
    }
    Ok(())
}

fn prepare_pass(record: &mut Migration, action: Action, phrase: &str) -> Result<()> {
    let run = record.run.as_mut().ok_or_else(action_unavailable)?;
    match action {
        Action::Cutover if run.status == Status::AwaitingCutover => {
            confirm(phrase, "CUTOVER")?;
            run.stage = Stage::Cutover;
        }
        Action::Resume if run.status == Status::Paused => {}
        Action::Retry if run.status == Status::Failed => {
            for bucket in &mut run.buckets {
                if bucket.status == BucketStatus::Failed {
                    bucket.status = BucketStatus::Pending;
                    bucket.error = None;
                }
            }
        }
        _ => return Err(action_unavailable()),
    }
    run.status = run.stage.status();
    Ok(())
}

fn cleanup_targets(record: &Migration, action: Action, phrase: &str) -> Result<Vec<usize>> {
    let run = record
        .run
        .as_ref()
        .filter(|run| {
            !matches!(
                run.status,
                Status::Copying | Status::CuttingOver | Status::AwaitingCutover
            )
        })
        .ok_or_else(action_unavailable)?;
    let source = action == Action::DeleteSources;
    let buckets: Vec<_> = run.buckets.iter().enumerate().filter(|(_, bucket)| {
        matches!(bucket.ownership, Ownership::Owned { .. }) && if source {
            bucket.status == BucketStatus::Verified && !bucket.source_deleted
                && record.private.effects.iter().any(|effect| matches!(effect,
                    super::model::Effect::Policy { side: super::model::Side::Source, bucket: name, .. } if name == &bucket.mapping.source))
        } else {
            !matches!(bucket.status, BucketStatus::Verified | BucketStatus::Removed)
        }
    }).collect();
    if buckets.is_empty() {
        return Err(action_unavailable());
    }
    let names = buckets
        .iter()
        .map(|(_, bucket)| {
            if source {
                bucket.mapping.source.as_str()
            } else {
                bucket.mapping.target.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    confirm(phrase, &format!("DELETE {names}"))?;
    Ok(buckets.into_iter().map(|(index, _)| index).collect())
}

fn confirm(actual: &str, expected: &str) -> Result<()> {
    if actual != expected {
        return Err(AppError::Invalid(
            "Confirm the exact migration action and scope.".into(),
        ));
    }
    Ok(())
}

fn action_unavailable() -> AppError {
    AppError::Invalid("This action is unavailable in the current migration state.".into())
}

fn busy() -> AppError {
    AppError::Invalid("Another migration action is running.".into())
}
