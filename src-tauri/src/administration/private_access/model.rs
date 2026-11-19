use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::model::Provider;

pub(super) const RESERVED_PREFIX: &str = "storeforge-private-";

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAccessDraft {
    pub name: String,
    pub browser: bool,
    pub manager: bool,
    pub groups: Vec<String>,
    pub policies: Vec<String>,
    pub inline_policies: Vec<InlinePolicy>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlinePolicy {
    pub name: String,
    pub document: String,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivateAccessStatus {
    Provisioning,
    Active,
    SetupIncomplete,
    Retiring,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAccess {
    pub id: Uuid,
    pub name: String,
    pub user_name: String,
    pub status: PrivateAccessStatus,
    pub issue: Option<String>,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
pub(super) struct Authority {
    pub connection: Uuid,
    pub endpoint: Uuid,
    pub tenant: String,
    pub account: String,
    pub provider: Provider,
    pub principal_key: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct ProvisioningRecord {
    pub view: PrivateAccess,
    pub authority: Authority,
    pub draft: PrivateAccessDraft,
    pub user_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum PrivateAccessChange {
    Create { draft: PrivateAccessDraft },
    Retry { id: Uuid },
    Retire { id: Uuid },
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct User {
    pub user_id: String,
    pub user_name: String,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct UserResult {
    pub user: User,
}

#[derive(Deserialize)]
#[serde(rename = "CreateUserResponse")]
pub(super) struct CreatedUser {
    #[serde(rename = "CreateUserResult")]
    pub result: UserResult,
}

#[derive(Deserialize)]
#[serde(rename = "GetUserResponse")]
pub(super) struct ReadUser {
    #[serde(rename = "GetUserResult")]
    pub result: UserResult,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct IssuedKey {
    pub user_name: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct KeyResult {
    pub access_key: IssuedKey,
}

#[derive(Deserialize)]
#[serde(rename = "CreateAccessKeyResponse")]
pub(super) struct CreatedKey {
    #[serde(rename = "CreateAccessKeyResult")]
    pub result: KeyResult,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct ListedKey {
    pub access_key_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct GroupMember {
    pub group_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct PolicyMember {
    pub policy_arn: String,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub(super) struct Members<T> {
    #[serde(default)]
    pub member: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct CleanupPage {
    pub is_truncated: bool,
    pub marker: Option<String>,
    pub access_key_metadata: Option<Members<ListedKey>>,
    pub groups: Option<Members<GroupMember>>,
    pub attached_policies: Option<Members<PolicyMember>>,
    pub policy_names: Option<Members<String>>,
}

#[derive(Deserialize)]
pub(super) struct CleanupResponse {
    #[serde(
        rename = "ListAccessKeysResult",
        alias = "ListGroupsForUserResult",
        alias = "ListAttachedUserPoliciesResult",
        alias = "ListUserPoliciesResult"
    )]
    pub result: CleanupPage,
}
