use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::error::{AppError, Result};

use super::model::{Effect, Mapping, Migration, Mode, Phase, Side};

#[derive(Clone, Copy, Deserialize, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Start,
    Cutover,
    Pause,
    Resume,
    Stop,
    Retry,
    RemoveIncomplete,
    DeleteSources,
    DeleteRecord,
}

#[derive(Clone, Copy, Deserialize, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Copying,
    CuttingOver,
    AwaitingCutover,
    Paused,
    Stopped,
    Failed,
    Verified,
}

#[derive(Clone, Copy, Deserialize, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BucketStatus {
    Pending,
    Preparing,
    Copying,
    AwaitingCutover,
    Verifying,
    Verified,
    Failed,
    Removed,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    PreCopy,
    Cutover,
}

impl Stage {
    pub fn status(self) -> Status {
        match self {
            Self::PreCopy => Status::Copying,
            Self::Cutover => Status::CuttingOver,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub status: Status,
    pub stage: Stage,
    pub buckets: Vec<BucketRun>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketRun {
    pub mapping: Mapping,
    pub status: BucketStatus,
    pub completed: usize,
    pub total: usize,
    pub current_key: Option<String>,
    pub error: Option<String>,
    pub cleanup_error: Option<String>,
    pub source_deleted: bool,
    pub ownership: Ownership,
    pub baseline: BTreeMap<String, Vec<Fingerprint>>,
    pub cleanup: Option<CleanupProof>,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Ownership {
    Pending,
    Creating { token: Uuid },
    Created { token: Uuid },
    Owned { token: Uuid },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Fingerprint {
    Marker,
    Data {
        sha256: String,
        tags: Vec<(String, String)>,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupProof {
    pub destination_digest: String,
    pub source_versions: BTreeMap<String, Vec<(String, Fingerprint)>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    status: Status,
    buckets: Vec<BucketView>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketView {
    mapping: Mapping,
    status: BucketStatus,
    completed: usize,
    total: usize,
    current_key: Option<String>,
    error: Option<String>,
    cleanup_error: Option<String>,
    source_deleted: bool,
    owned: bool,
    source_protected: bool,
}

impl Run {
    pub fn start(record: &Migration) -> Result<Self> {
        record.require_recovered()?;
        let report = record.report.as_ref().filter(|report| {
            report.passed(record.revision, Phase::Active)
                && report
                    .pairs
                    .iter()
                    .map(|plan| &plan.mapping)
                    .eq(&record.draft.mappings)
        });
        if report.is_none() || record.run.is_some() {
            return Err(AppError::Invalid(
                "Review fresh active checks before starting copy.".into(),
            ));
        }
        let stage = match record.draft.mode {
            Mode::PreCopy => Stage::PreCopy,
            Mode::Immediate => Stage::Cutover,
        };
        Ok(Self {
            status: stage.status(),
            stage,
            buckets: record
                .draft
                .mappings
                .iter()
                .cloned()
                .map(BucketRun::new)
                .collect(),
        })
    }

    pub fn view(&self, effects: &[Effect]) -> RunView {
        RunView {
            status: self.status,
            buckets: self
                .buckets
                .iter()
                .map(|bucket| BucketView {
                    mapping: bucket.mapping.clone(),
                    status: bucket.status,
                    completed: bucket.completed,
                    total: bucket.total,
                    current_key: bucket.current_key.clone(),
                    error: bucket.error.clone(),
                    cleanup_error: bucket.cleanup_error.clone(),
                    source_deleted: bucket.source_deleted,
                    owned: matches!(bucket.ownership, Ownership::Owned { .. }),
                    source_protected: effects.iter().any(|effect| matches!(effect,
                        Effect::Policy { side: Side::Source, bucket: source, .. } if source == &bucket.mapping.source)),

                })
                .collect(),
        }
    }

    pub fn reconcile_after_restart(&mut self) {
        if matches!(self.status, Status::Copying | Status::CuttingOver) {
            self.status = Status::Paused;
        }
    }

    pub fn finish(&mut self) {
        self.status = if self
            .buckets
            .iter()
            .any(|bucket| bucket.status == BucketStatus::Failed)
        {
            Status::Failed
        } else if self
            .buckets
            .iter()
            .any(|bucket| bucket.status == BucketStatus::Removed)
        {
            Status::Stopped
        } else if self.stage == Stage::PreCopy {
            Status::AwaitingCutover
        } else {
            Status::Verified
        };
    }
}

impl BucketRun {
    fn new(mapping: Mapping) -> Self {
        Self {
            mapping,
            status: BucketStatus::Pending,
            completed: 0,
            total: 0,
            current_key: None,
            error: None,
            cleanup_error: None,
            source_deleted: false,
            ownership: Ownership::Pending,
            baseline: BTreeMap::new(),
            cleanup: None,
        }
    }
}
