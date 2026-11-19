use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PortalRead {
    Projects,
    State,
    Usage,
    Trends,
    Health,
    Alerts,
    Activity,
    Spaces,
    Space,
    Statistics,
    SpaceSettings,
    Files {
        prefix: String,
        cursor: Option<String>,
    },
    Detail {
        key: String,
    },
    Trash {
        key_marker: Option<String>,
        version_id_marker: Option<String>,
    },
    Versions {
        key: String,
        key_marker: Option<String>,
        version_id_marker: Option<String>,
    },
    Candidates {
        search: String,
        page: u64,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PortalChange {
    Create { draft: SpaceCreate },
    Import { draft: SpaceImport },
    Edit { draft: SpaceEdit },
    Archive { archived: bool },
    TakeOwnership,
    Delete { confirmation: String },
    Icon { draft: IconChoice },
    SpaceSettings { draft: SpaceSettings },
    Folder { prefix: String },
    DeleteFile { key: String },
    RestoreVersion { key: String, version_id: String },
    RestorePrefix { prefix: String },
    Cleanup { confirmation: String },
}

#[derive(Deserialize, Serialize)]
pub struct SpaceCreate {
    name: String,
    naming_mode: NamingMode,
    #[serde(flatten)]
    access: SpaceProvisioning,
}

#[derive(Deserialize, Serialize)]
pub struct SpaceImport {
    bucket_name: String,
    #[serde(flatten)]
    access: SpaceProvisioning,
}

#[derive(Deserialize, Serialize)]
struct SpaceProvisioning {
    description: Option<String>,
    visibility: Visibility,
    share_scope: ShareScope,
    account_member_role: Option<GrantRole>,
    initial_shares: Vec<InitialShare>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum NamingMode {
    GenericUuid,
    NamedBucket,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Visibility {
    Private,
    Shared,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum ShareScope {
    Restricted,
    Account,
}

#[derive(Deserialize, Serialize)]
enum GrantRole {
    Viewer,
    Editor,
}

#[derive(Deserialize, Serialize)]
struct InitialShare {
    user_id: u64,
    role: GrantRole,
}

#[derive(Deserialize, Serialize)]
pub struct SpaceEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    description: String,
}

#[derive(Deserialize, Serialize)]
pub struct IconChoice {
    source: IconSource,
    preset: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum IconSource {
    Preset,
}

#[derive(Deserialize, Serialize)]
pub struct SpaceSettings {
    versioning_enabled: bool,
    lifecycle_enabled: bool,
    version_history_retention_days: u64,
}

#[derive(Deserialize)]
pub struct SpaceAccess {
    pub name: String,
    pub role: SpaceRole,
    pub status: SpaceStatus,
    pub can_browse: bool,
    pub can_delete: bool,
    pub internal_bucket_name: Option<String>,
}

#[derive(Deserialize)]
pub enum SpaceRole {
    Viewer,
    Editor,
    Owner,
    Manager,
}

#[derive(Deserialize, PartialEq, Eq)]
pub enum SpaceStatus {
    Active,
    Attention,
    Archived,
}
