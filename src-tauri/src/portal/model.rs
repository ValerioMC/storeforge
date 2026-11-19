use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PortalRead {
    Projects,
    Governance {
        view: GovernanceRead,
    },
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
    Governance { action: GovernanceChange },
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

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GovernanceRead {
    Settings,
    History { space_id: Option<String> },
    AccessLogs { query: AccessLogQuery },
    Composition,
    Traffic { window: TrafficWindow },
    Cost { month: String },
}

#[derive(Deserialize, Serialize)]
pub struct AccessLogQuery {
    pub date: chrono::NaiveDate,
    pub space_id: Option<String>,
    pub offset: u64,
    pub timezone_offset_minutes: i32,
    pub filter: Option<AccessLogFilter>,
}

#[derive(Deserialize, Serialize)]
pub struct AccessLogFilter {
    pub field: AccessLogField,
    pub value: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessLogField {
    Action,
    Space,
    Path,
    Identity,
    Result,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TrafficWindow {
    Hour,
    Day,
    Week,
    Month,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GovernanceChange {
    SaveSetting {
        setting: SettingChange,
    },
    RequestSetting {
        setting: SettingChange,
        reason: Option<String>,
    },
    RequestQuota {
        direction: QuotaDirection,
        size: f64,
        unit: QuotaUnit,
        reason: Option<String>,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "key", content = "value")]
pub enum SettingChange {
    #[serde(rename = "cost_estimation_enabled")]
    CostEstimation(Option<bool>),
    #[serde(rename = "browser_access_enabled")]
    BrowserAccess(Option<bool>),
    #[serde(rename = "allow_private_storage_space_create")]
    PrivateSpaces(Option<bool>),
    #[serde(rename = "allow_portal_named_bucket_create")]
    NamedBuckets(Option<bool>),
    #[serde(rename = "allow_portal_user_access_key_create")]
    PersonalKeys(Option<bool>),
    #[serde(rename = "allow_portal_user_external_sharing")]
    ExternalSharing(Option<bool>),
    #[serde(rename = "server_access_logging_enabled")]
    ServerAccessLogging(Option<bool>),
    #[serde(rename = "storage_space_version_cleanup_enabled")]
    VersionCleanup(Option<bool>),
    #[serde(rename = "bucket_defaults.versioning")]
    DefaultVersioning(Option<bool>),
    #[serde(rename = "bucket_defaults.enable_lifecycle")]
    DefaultLifecycle(Option<bool>),
    #[serde(rename = "bucket_defaults.enable_cors")]
    DefaultCors(Option<bool>),
    #[serde(rename = "bucket_defaults.noncurrent_version_expiration_days")]
    DefaultRetention(Option<u32>),
    #[serde(rename = "bucket_defaults.cors_allowed_origins")]
    DefaultCorsOrigins(Option<Vec<String>>),
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaDirection {
    Increase,
    Decrease,
}

#[derive(Deserialize, Serialize)]
pub enum QuotaUnit {
    MiB,
    GiB,
    TiB,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestStatus {
    Pending,
    Processing,
    Approved,
    Rejected,
    Failed,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestType {
    PortalUserAccess,
    PortalUserRemoval,
    AccountQuotaChange,
    PortalSettingChange,
}

#[derive(Deserialize, Serialize)]
pub struct AdminRequestQuery {
    pub page: u64,
    pub page_size: u64,
    pub search: String,
    pub status: Option<RequestStatus>,
    pub request_type: Option<RequestType>,
    pub account_id: Option<u64>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AdminResponse {
    Approve { message: Option<String> },
    Reject { message: Option<String> },
    Message { message: String },
}

#[derive(Deserialize, Serialize)]
pub struct LogExport {
    pub date_from: chrono::NaiveDate,
    pub date_to: chrono::NaiveDate,
    pub timezone_offset_minutes: i32,
    pub space_id: Option<String>,
}
