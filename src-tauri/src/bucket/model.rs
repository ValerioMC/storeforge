use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub name: String,
    pub created_at: Option<String>,
}

#[derive(Clone, Copy, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Configuration {
    Versioning,
    Lifecycle,
    Cors,
    Policy,
    Acl,
    OwnershipControls,
    PublicAccessBlock,
    Encryption,
    ObjectLock,
    Tagging,
    Logging,
    Notification,
    Replication,
    Website,
    RequestPayment,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "action", content = "document")]
pub enum ConfigurationChange {
    Read,
    Write(String),
    Remove,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketCreation {
    pub versioning_error: Option<String>,
}

#[derive(Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketQuota {
    pub enabled: bool,
    pub max_bytes: String,
    pub max_objects: String,
}

#[derive(Deserialize)]
pub struct BucketQuotaUpdate {
    pub expected: BucketQuota,
    pub draft: BucketQuota,
}
