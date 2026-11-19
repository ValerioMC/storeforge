use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileScope {
    pub connection: Uuid,
    pub revision: Option<Uuid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    Available,
    Denied,
    Missing,
    Unsupported,
    Unavailable,
    Stale,
    NotApplicable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation<T> {
    pub availability: Availability,
    pub status: Option<u16>,
    pub data: Option<T>,
}

impl<T> Observation<T> {
    pub fn absent(availability: Availability, status: Option<u16>) -> Self {
        Self {
            availability,
            status,
            data: None,
        }
    }

    pub fn available(data: T, status: u16) -> Self {
        Self {
            availability: Availability::Available,
            status: Some(status),
            data: Some(data),
        }
    }

    pub fn without_data<U>(&self) -> Observation<U> {
        Observation::absent(self.availability, self.status)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct LocalZone {
    pub id: String,
    pub name: String,
    pub realm_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Realm {
    pub id: String,
    pub name: String,
    pub current_period: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    pub id: String,
    pub realm_id: String,
    pub epoch: Option<String>,
    pub realm_epoch: Option<String>,
    pub master_zone: Option<String>,
    pub zones: Vec<Zone>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub group_id: String,
    pub group_name: String,
    pub read_only: Option<bool>,
    pub sync_from_all: Option<bool>,
    pub sync_from: Vec<String>,
    pub policy_configured: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredZone {
    pub scope: ProfileScope,
    pub name: String,
    pub local: Observation<LocalZone>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Topology {
    pub anchor: ProfileScope,
    pub collected_at: String,
    pub realm: Observation<Realm>,
    pub period: Observation<Period>,
    pub registrations: Vec<RegisteredZone>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ZoneBinding {
    pub scope: ProfileScope,
    pub zone: String,
    pub realm: String,
    pub period: String,
    pub epoch: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowFamily {
    Metadata,
    Data,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FlowScope {
    pub destination: ZoneBinding,
    pub family: FlowFamily,
    pub source_zone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub phase: String,
    pub expected_shards: Option<u32>,
    pub generation: Option<String>,
    pub period: Option<String>,
    pub shards: Vec<Shard>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shard {
    pub id: u32,
    pub phase: String,
    pub marker: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BucketSelector {
    pub name: String,
    pub tenant: String,
    pub instance: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketIdentity {
    pub name: String,
    pub tenant: String,
    pub instance: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplicationScope {
    pub source: ZoneBinding,
    pub destination: ZoneBinding,
    pub source_bucket: BucketSelector,
    pub destination_bucket: BucketSelector,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Replication {
    pub source: Observation<BucketIdentity>,
    pub destination: Observation<BucketIdentity>,
    pub sync: Observation<SyncStatus>,
    pub bilog: Observation<BilogInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BilogInfo {
    pub latest: Option<String>,
    pub oldest: Option<String>,
    pub generations: Vec<Generation>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generation {
    pub generation: String,
    pub shards: u32,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Journal {
    Metadata {
        period: String,
        shard: u32,
    },
    Data {
        shard: u32,
    },
    Bilog {
        bucket: BucketSelector,
        generation: String,
        shard: Option<u32>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JournalScope {
    pub target: ZoneBinding,
    pub journal: Journal,
    pub marker: Option<String>,
    pub limit: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalPage {
    pub entries: Vec<JournalEntry>,
    pub next_marker: Option<String>,
    pub next_generation: Option<Generation>,
    pub last_update: Option<String>,
    pub bucket: Option<BucketIdentity>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub id: Option<String>,
    pub key: Option<String>,
    pub timestamp: Option<String>,
    pub category: Option<String>,
    pub operation: Option<String>,
    pub generation: Option<String>,
    pub instance: Option<String>,
}
