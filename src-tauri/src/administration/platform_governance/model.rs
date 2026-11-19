use serde::{Deserialize, Serialize};

use crate::connection::shared_service::model::ServiceRole;

#[derive(Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct GovernancePage<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub total: u64,
    pub has_next: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct ListingQuery {
    pub page: u32,
    pub search: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct AccessFilters {
    pub search: Option<String>,
    pub scope: Option<AccessScope>,
    pub source: Option<GrantSourceKind>,
    pub right: Option<RightCode>,
    pub user_id: Option<i64>,
    pub group_id: Option<i64>,
    pub target_id: Option<i64>,
}

#[derive(Serialize)]
pub struct AccessQuery<'a> {
    pub page: u32,
    #[serde(flatten)]
    pub filters: &'a AccessFilters,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessScope {
    Platform,
    RgwAccount,
    RgwUser,
    S3Connection,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantSourceKind {
    Direct,
    Group,
    Resource,
    Platform,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RightCode {
    CephAdmin,
    StorageOps,
    ManagerBucketCompare,
    ManagerBucketIntegrityCheck,
    ManagerBucketMigration,
    ManagerFeatureRules,
    ManagerBucketPurge,
    PrivateConnectionCreate,
    ManagedPrivateConnectionProvision,
    BrowserAdvancedFeatures,
    AccountAdministrator,
    PortalManager,
    PortalUser,
    ManagerBrowserDataAccess,
    RgwUserAccess,
    SharedConnectionAccess,
}

#[derive(Deserialize, Serialize)]
pub struct AccessRow {
    pub key: String,
    pub principal: AccessPrincipal,
    pub target: AccessTarget,
    pub scope: AccessScope,
    pub rights: Vec<AccessRight>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct AccessPrincipal {
    pub id: i64,
    pub email: String,
    pub full_name: Option<String>,
    pub role: ServiceRole,
    pub is_active: bool,
}

#[derive(Deserialize, Serialize)]
pub struct AccessTarget {
    pub id: Option<i64>,
    pub name: String,
    pub identifier: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct AccessRight {
    pub code: RightCode,
    pub sources: Vec<GrantSource>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct GrantSource {
    pub kind: GrantSourceKind,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub system_key: Option<String>,
    pub user_details: Vec<GroupMember>,
    pub account_details: Vec<NamedResource>,
    pub s3_user_details: Vec<NamedResource>,
    pub s3_connection_details: Vec<NamedResource>,
    pub openstack_cloud_details: Vec<NamedResource>,
}

#[derive(Deserialize, Serialize)]
pub struct NamedResource {
    pub id: i64,
    pub name: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct GroupMember {
    pub id: i64,
    pub email: String,
    pub full_name: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct GroupDraft {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct ManagedConnection {
    pub id: i64,
    pub name: String,
    pub endpoint_url: String,
    pub is_active: bool,
    pub execution_status: ConnectionReadiness,
    pub created_by_email: Option<String>,
    pub created_by_full_name: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionReadiness {
    Ready,
    RemediationRequired,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct PersonalConnection {
    pub id: i64,
    pub name: String,
    pub endpoint_url: String,
    pub is_shared: bool,
    pub is_active: bool,
    pub server_managed: bool,
}

#[derive(Serialize)]
pub struct ConnectionName<'a> {
    pub name: &'a str,
}
