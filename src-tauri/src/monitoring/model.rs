use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::model::{Connection, IdentityKind};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub connection_id: Uuid,
    pub revision: Option<Uuid>,
    pub endpoint_id: Uuid,
    pub tenant: String,
    pub account: String,
    pub region: String,
}

impl From<&Connection> for Scope {
    fn from(profile: &Connection) -> Self {
        Self {
            connection_id: profile.id,
            revision: profile.revision,
            endpoint_id: profile.endpoint_id(),
            tenant: profile.settings.tenant.clone(),
            account: profile.settings.account.clone(),
            region: profile.region.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Coverage {
    Complete,
    CurrentOnly,
    Partial,
    Denied,
    Unavailable,
    Missing,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading<T> {
    pub coverage: Coverage,
    pub value: Option<T>,
    pub detail: Option<String>,
}

impl<T> Reading<T> {
    pub fn new(coverage: Coverage, value: Option<T>, detail: Option<String>) -> Self {
        Self {
            coverage,
            value,
            detail,
        }
    }

    pub fn complete(value: T) -> Self {
        Self::new(Coverage::Complete, Some(value), None)
    }

    pub fn missing(coverage: Coverage, detail: impl Into<String>) -> Self {
        Self::new(coverage, None, Some(detail.into()))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    S3Listing,
    RgwUsage,
    RgwBucket,
    Http,
    SignedS3,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub source: Source,
    pub identity: Option<IdentityKind>,
    pub principal_id: Option<Uuid>,
}

impl Origin {
    pub fn new(profile: &Connection, source: Source, identity: Option<IdentityKind>) -> Self {
        Self {
            source,
            identity,
            principal_id: identity.and_then(|kind| profile.principal_id(kind)),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionValue {
    pub objects: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketUsage {
    pub current_bytes: Option<u64>,
    pub noncurrent_bytes: Option<u64>,
    pub current_objects: Option<u64>,
    pub noncurrent_objects: Option<u64>,
    pub delete_markers: Option<u64>,
    pub sizes: BTreeMap<String, DistributionValue>,
    pub ages: BTreeMap<String, DistributionValue>,
    pub types: BTreeMap<String, DistributionValue>,
    pub storage_classes: BTreeMap<String, DistributionValue>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketSample {
    pub bucket: String,
    pub collected_at: DateTime<Utc>,
    pub usage: Reading<BucketUsage>,
    pub quota: Reading<Quota>,
    pub rgw_storage: Reading<RgwStorage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub enabled: bool,
    pub max_bytes: Option<u64>,
    pub max_objects: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RgwStorage {
    pub bytes: u64,
    pub objects: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Traffic {
    pub bytes_sent: Option<u64>,
    pub bytes_received: Option<u64>,
    pub requests: Option<u64>,
    pub successful_requests: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficPeriod {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub totals: Traffic,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSample {
    pub collected_at: DateTime<Utc>,
    pub listing_origin: Origin,
    pub quota_origin: Origin,
    pub traffic_origin: Origin,
    pub bucket_coverage: Reading<u64>,
    pub buckets: Vec<BucketSample>,
    pub traffic: Reading<TrafficPeriod>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagePoint {
    pub collected_at: DateTime<Utc>,
    pub coverage: Coverage,
    pub current_bytes: Option<u64>,
    pub current_objects: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySlot {
    pub period_start: DateTime<Utc>,
    pub sample: Option<UsagePoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckMode {
    Http,
    Signed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    Up,
    Slow,
    Down,
    Denied,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthSample {
    pub checked_at: DateTime<Utc>,
    pub origin: Origin,
    pub mode: CheckMode,
    pub status: HealthStatus,
    pub latency_ms: Option<u64>,
    pub baseline_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Incident {
    pub mode: CheckMode,
    pub opened_at: DateTime<Utc>,
    pub recovered_at: Option<DateTime<Utc>>,
    pub severity: HealthStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringView {
    pub scope: Scope,
    pub retrieved_at: DateTime<Utc>,
    pub stale: bool,
    pub usage: Option<UsageSample>,
    pub hourly: Vec<HistorySlot>,
    pub daily: Vec<HistorySlot>,
    pub health: Vec<HealthSample>,
    pub incidents: Vec<Incident>,
}
