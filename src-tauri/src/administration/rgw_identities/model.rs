use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::model::Connection;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserIdentity {
    pub tenant: String,
    pub uid: String,
}

impl UserIdentity {
    pub fn qualified(&self) -> String {
        if self.tenant.is_empty() {
            self.uid.clone()
        } else {
            format!("{}${}", self.tenant, self.uid)
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IdentityTarget {
    Account { id: String },
    User { identity: UserIdentity },
}

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum IdentityKind {
    Account,
    User,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryRow {
    pub target: IdentityTarget,
    pub managed: bool,
    pub root: Option<UserIdentity>,
}

#[derive(Serialize)]
pub struct InventoryPage {
    pub items: Vec<InventoryRow>,
    pub marker: Option<String>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountLimits {
    pub max_users: Option<i32>,
    pub max_buckets: Option<i32>,
    pub max_roles: Option<i32>,
    pub max_groups: Option<i32>,
    pub max_access_keys: Option<i32>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub enabled: bool,
    pub max_bytes: Option<i64>,
    pub max_objects: Option<i64>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Usage {
    pub bytes: Option<u64>,
    pub objects: Option<u64>,
    pub buckets: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDetail {
    pub id: String,
    pub tenant: String,
    pub name: String,
    pub email: String,
    pub limits: AccountLimits,
    pub quota: Option<Quota>,
    pub bucket_quota: Option<Quota>,
    pub usage: Option<Usage>,
    pub user_count: Option<u64>,
    pub root: Option<UserIdentity>,
    #[serde(skip)]
    pub users: Option<Vec<UserIdentity>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDetail {
    pub identity: UserIdentity,
    pub display_name: String,
    pub email: String,
    pub account: Option<String>,
    pub account_root: Option<bool>,
    pub suspended: Option<bool>,
    pub max_buckets: Option<i32>,
    pub quota: Option<Quota>,
    pub bucket_quota: Option<Quota>,
    pub usage: Option<Usage>,
    pub keys: Option<Vec<AccessKey>>,
    pub connection_key_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessKey {
    pub id: String,
    pub active: Option<bool>,
    pub metadata: KeyMetadata,
    pub enforcement: Option<Enforcement>,
    pub private_profile: Option<Uuid>,
    pub issue: Option<String>,
    pub provider_missing: bool,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyMetadata {
    pub name: String,
    pub notes: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Enforcement {
    Pending,
    Disabled,
    Failed { message: String },
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
pub struct RegistryScope {
    pub endpoint: Uuid,
    pub admin_url: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Registration {
    pub scope: RegistryScope,
    pub target: IdentityTarget,
    pub root: Option<UserIdentity>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct KeyRecord {
    pub scope: RegistryScope,
    pub owner: UserIdentity,
    pub id: String,
    pub administrator: Uuid,
    pub metadata: KeyMetadata,
    pub enforcement: Option<Enforcement>,
    pub private_profile: Option<Uuid>,
    pub issue: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
pub struct Registry {
    pub identities: Vec<Registration>,
    pub keys: Vec<KeyRecord>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountImport {
    pub id: String,
    pub root: Option<UserIdentity>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDraft {
    pub name: String,
    pub email: Option<String>,
    pub limits: AccountLimits,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountCreation {
    pub id: Option<String>,
    pub tenant: String,
    pub draft: AccountDraft,
    pub root_uid: Option<String>,
    pub managed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDraft {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub suspended: Option<bool>,
    pub max_buckets: Option<i32>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyScope {
    Inventory,
    Delegated,
}

#[derive(Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReadRequest {
    List {
        kind: IdentityKind,
        managed: bool,
        marker: Option<String>,
    },
    Account {
        id: String,
    },
    AccountUsage {
        id: String,
    },
    User {
        identity: UserIdentity,
    },
    DelegatedKeys,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ReadResponse {
    List(InventoryPage),
    Account(AccountDetail),
    Usage(Usage),
    User(UserDetail),
}

#[derive(Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChangeRequest {
    ImportAccounts {
        entries: Vec<AccountImport>,
    },
    ImportUsers {
        identities: Vec<UserIdentity>,
    },
    CreateAccount(AccountCreation),
    UpdateAccount {
        id: String,
        draft: AccountDraft,
    },
    CreateUser {
        identity: UserIdentity,
        draft: UserDraft,
        account: Option<String>,
        root: bool,
        managed: bool,
    },
    UpdateUser {
        identity: UserIdentity,
        draft: UserDraft,
    },
    SetQuota {
        target: IdentityTarget,
        bucket: bool,
        quota: Quota,
    },
    Key {
        scope: KeyScope,
        owner: UserIdentity,
        change: KeyChange,
    },
}

#[derive(Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum KeyChange {
    Create {
        metadata: KeyMetadata,
        private: bool,
    },
    Metadata {
        id: String,
        metadata: KeyMetadata,
    },
    Status {
        id: String,
        active: bool,
    },
    Delete {
        id: String,
    },
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeResponse {
    pub target: Option<IdentityTarget>,
    pub issued: Option<IssuedKey>,
    pub profile: Option<Connection>,
    pub issue: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuedKey {
    pub access_key: String,
    pub secret_key: String,
}
