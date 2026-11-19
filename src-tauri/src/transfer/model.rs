use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::object::model::{Location, MetadataUpdate, ObjectId, RetentionMode, Tag};

use super::maintenance::{IntegrityRequest, MaintenanceReport, PurgePreview};

#[derive(Deserialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum Transfer {
    Upload {
        target: Location,
        paths: Vec<PathBuf>,
        overwrite: bool,
    },
    Download {
        connection: Uuid,
        objects: Vec<ObjectId>,
        target: PathBuf,
        directory: bool,
    },
    Archive {
        connection: Uuid,
        objects: Vec<ObjectId>,
        #[serde(default)]
        folders: Vec<String>,
        #[serde(default)]
        prefix: String,
        bucket: Option<String>,
        target: PathBuf,
    },
    Delete {
        connection: Uuid,
        objects: Vec<ObjectId>,
    },
    Tag {
        connection: Uuid,
        objects: Vec<ObjectId>,
        tags: Vec<Tag>,
    },
    Copy {
        source: Location,
        objects: Vec<ObjectId>,
        #[serde(default)]
        folders: Vec<String>,
        rename_to: Option<String>,
        target: Location,
        move_source: bool,
        overwrite: bool,
    },
    Attributes {
        source: Location,
        objects: Vec<ObjectId>,
        folders: Vec<String>,
        update: BulkAttributes,
    },
    ApplyVersionPlan {
        source: Location,
        rule: VersionRule,
        items: Vec<VersionPlanItem>,
    },
    Migrate {
        source: Location,
        target: Location,
        overwrite: bool,
    },
    Integrity {
        request: IntegrityRequest,
    },
    Purge {
        preview: PurgePreview,
        confirmation: String,
    },
}

impl Transfer {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Upload { .. } => "Upload",
            Self::Download { .. } => "Download",
            Self::Archive { .. } => "Download ZIP",
            Self::Delete { .. } => "Delete objects",
            Self::Tag { .. } => "Update object tags",
            Self::Copy {
                move_source: false, ..
            } => "Copy objects",
            Self::Copy {
                move_source: true, ..
            } => "Move objects",
            Self::Attributes { .. } => "Update bulk attributes",
            Self::ApplyVersionPlan { .. } => "Apply version plan",
            Self::Migrate { .. } => "Migrate prefix",
            Self::Integrity { .. } => "Bucket integrity",
            Self::Purge { .. } => "Purge buckets",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkAttributes {
    pub metadata: Option<MetadataUpdate>,
    pub tags: Option<Vec<Tag>>,
    pub storage_class: Option<String>,
    pub acl: Option<String>,
    pub legal_hold: Option<bool>,
    pub retention: Option<BulkRetention>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkRetention {
    pub mode: RetentionMode,
    pub until: String,
    pub bypass_governance: bool,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VersionRule {
    RestoreDeleted,
    RestoreAt {
        at: DateTime<Utc>,
        delete_missing: bool,
    },
    Cleanup {
        keep: Option<usize>,
        before: Option<DateTime<Utc>>,
        orphan_markers: bool,
    },
}

#[derive(Clone, Copy, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VersionAction {
    Restore,
    MarkDeleted,
    DeleteVersion,
}

#[derive(Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionPlanItem {
    pub key: String,
    pub version_id: String,
    pub expected_latest: String,
    pub action: VersionAction,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFailure {
    pub item: String,
    pub message: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: Uuid,
    pub label: String,
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub failures: Vec<JobFailure>,
    #[serde(default)]
    pub successful_items: Vec<String>,
    #[serde(default)]
    pub bytes_processed: u64,
    #[serde(default)]
    pub current_bytes: u64,
    pub current: Option<String>,
    pub cancel_requested: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance: Option<MaintenanceReport>,
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Difference {
    SourceOnly,
    TargetOnly,
    Changed,
    DifferentEtag,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonRow {
    pub key: String,
    pub difference: Difference,
    pub source_bytes: Option<i64>,
    pub target_bytes: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub matching: usize,
    pub changed: usize,
    pub different_etag: usize,
    pub source_only: usize,
    pub target_only: usize,
    pub differences: Vec<ComparisonRow>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub objects: usize,
    pub bytes: u64,
    pub versions: usize,
    pub delete_markers: usize,
}
