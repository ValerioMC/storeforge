use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PortalRead {
    Projects,
    Sharing {
        view: SharingRead,
    },
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
    Sharing { action: SharingChange },
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
pub enum ShareScope {
    Restricted,
    Account,
}

#[derive(Deserialize, Serialize)]
pub enum GrantRole {
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

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SharingRead {
    Collaborators,
    Requests,
    Collaborator { user_id: u64 },
    Access,
    Links,
    Keys,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SharingChange {
    Audience {
        share_scope: ShareScope,
        account_member_role: Option<GrantRole>,
    },
    Grant {
        user_id: u64,
        role: GrantRole,
    },
    ChangeRole {
        user_id: u64,
        role: GrantRole,
    },
    Revoke {
        user_id: u64,
    },
    CreateLink {
        object_key: String,
        label: Option<String>,
        expires_at: Option<String>,
    },
    RevokeLink {
        link_id: u64,
    },
    PersonalKey,
    ExternalKey {
        storage_space_id: String,
        external_email: String,
        permission: KeyPermission,
    },
    KeyStatus {
        key_id: String,
        active: bool,
    },
    DeleteKey {
        key_id: String,
    },
    AddMember {
        target_name: String,
        target_email: String,
        portal_role: ProjectRole,
        reason: Option<String>,
    },
    RemoveMember {
        target_email: String,
        target_name: Option<String>,
        reason: Option<String>,
    },
    MemberRole {
        target_user_id: u64,
        portal_role: ProjectRole,
        reason: Option<String>,
    },
}

#[derive(Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum KeyPermission {
    ReadOnly,
    ReadWrite,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRole {
    PortalUser,
    PortalManager,
}

#[derive(Deserialize, Serialize)]
pub struct AccessKeys {
    pub s3_endpoint: Option<String>,
    pub force_path_style: bool,
    pub access_keys: Vec<AccessKey>,
    pub can_manage_access_keys: bool,
    pub can_create_external_access: bool,
    pub max_access_keys: u64,
}

#[derive(Deserialize, Serialize)]
pub struct AccessKey {
    pub access_key_id: String,
    pub target_type: AccessKeyTarget,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub is_active: bool,
    pub is_portal: bool,
    pub deletable: bool,
    pub expires_at: Option<String>,
    pub external_email: Option<String>,
    pub storage_space_name: Option<String>,
    pub bucket_name: Option<String>,
    pub permission: Option<KeyPermission>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccessKeyTarget {
    #[serde(rename = "self")]
    Personal,
    External,
}

#[derive(Deserialize, Serialize)]
pub struct KeyReceipt {
    #[serde(flatten)]
    pub key: AccessKey,
    pub secret_access_key: Option<String>,
}
