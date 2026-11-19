use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectId {
    pub bucket: String,
    pub key: String,
    pub version_id: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub connection: Uuid,
    pub bucket: String,
    pub prefix: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectQuery {
    pub location: Location,
    pub cursor: Option<String>,
    pub recursive: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRow {
    pub key: String,
    pub folder: bool,
    pub size: Option<i64>,
    pub modified: Option<String>,
    pub storage_class: Option<String>,
    pub etag: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectPage {
    pub rows: Vec<ObjectRow>,
    pub cursor: Option<String>,
}

#[derive(Clone, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionCursor {
    pub key: String,
    pub version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectVersion {
    pub key: String,
    pub version_id: String,
    pub latest: bool,
    pub delete_marker: bool,
    pub size: Option<i64>,
    pub modified: Option<String>,
    pub storage_class: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionPage {
    pub versions: Vec<ObjectVersion>,
    pub cursor: Option<VersionCursor>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDetails {
    pub content_type: Option<String>,
    pub size: Option<i64>,
    pub etag: Option<String>,
    pub modified: Option<String>,
    pub version_id: Option<String>,
    pub storage_class: Option<String>,
    pub retain_until: Option<String>,
    pub retention_mode: Option<String>,
    pub legal_hold: Option<String>,
    pub encryption: Option<String>,
    pub restore: Option<String>,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub content_encoding: Option<String>,
    pub content_language: Option<String>,
    pub expires: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Tag {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataUpdate {
    pub content_type: Option<String>,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub content_encoding: Option<String>,
    pub content_language: Option<String>,
    pub expires: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub content_type: String,
    pub kind: PreviewKind,
    pub data: String,
    pub truncated: bool,
    pub oversized: bool,
}

#[derive(PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PreviewKind {
    Text,
    Image,
    Audio,
    Video,
    Pdf,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RetentionMode {
    Governance,
    Compliance,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ObjectChange {
    Delete,
    RestoreVersion,
    SetTags { tags: Vec<Tag> },
    SetMetadata { update: MetadataUpdate },
    SetAcl { acl: String },
    LegalHold { enabled: bool },
    Retention { mode: RetentionMode, until: String },
    RestoreArchive { days: i32, tier: String },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultipartUpload {
    pub key: String,
    pub upload_id: String,
    pub initiated: Option<String>,
}
