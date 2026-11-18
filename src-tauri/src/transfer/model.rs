use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::object::model::{Location, ObjectId, Tag};

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
        target: Location,
        move_source: bool,
        overwrite: bool,
    },
    Migrate {
        source: Location,
        target: Location,
        overwrite: bool,
    },
    Purge {
        source: Location,
        include_versions: bool,
        remove_bucket: bool,
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
            Self::Migrate { .. } => "Migrate prefix",
            Self::Purge { .. } => "Purge objects",
        }
    }
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
    pub current: Option<String>,
    pub cancel_requested: bool,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Integrity {
    pub checked: usize,
    pub failed: usize,
    pub failures: Vec<JobFailure>,
}
