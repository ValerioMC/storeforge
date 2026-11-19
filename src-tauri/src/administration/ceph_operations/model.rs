use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bucket::workbench::BucketTarget;

use super::super::rgw_identities::model::UserIdentity;

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Action {
    Bucket {
        target: BucketTarget,
        owner: Option<String>,
        change: BucketChange,
        confirmation: String,
    },
    DeleteUser {
        identity: UserIdentity,
        account: Option<String>,
        purge_data: bool,
        confirmation: String,
    },
    DeleteAccount {
        id: String,
        tenant: String,
        confirmation: String,
    },
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BucketChange {
    Delete {
        purge_objects: bool,
        bypass_gc: bool,
    },
    Link {
        destination: BucketOwner,
    },
    Unlink,
    Index {
        repair: bool,
        check_objects: bool,
    },
    Browse,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum BucketOwner {
    Account {
        id: String,
        tenant: String,
    },
    User {
        identity: UserIdentity,
        account: Option<String>,
    },
}

impl BucketOwner {
    pub fn label(&self) -> String {
        match self {
            Self::Account { id, .. } => id.clone(),
            Self::User { identity, .. } => identity.qualified(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub success: bool,
    pub status: Option<u16>,
    pub code: Option<String>,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexOutcome {
    pub target: BucketTarget,
    pub result: Outcome,
}
