use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::administration::rgw_identities::model::{IdentityTarget, RegistryScope};

use super::super::model::{Reading, Traffic};

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rates {
    pub storage: f64,
    pub ingress: f64,
    pub egress: f64,
    pub requests: f64,
}

impl Rates {
    pub fn valid(self) -> bool {
        [self.storage, self.ingress, self.egress, self.requests]
            .into_iter()
            .all(|rate| (0.0..=999_999.999_999).contains(&rate))
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub currency: String,
    pub rates: Option<Rates>,
    pub overrides: BTreeMap<Uuid, Rates>,
    pub collect_storage: bool,
    pub collect_traffic: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            currency: "USD".into(),
            rates: None,
            overrides: BTreeMap::new(),
            collect_storage: true,
            collect_traffic: true,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub day: NaiveDate,
    pub collected_at: DateTime<Utc>,
    pub endpoint_id: Uuid,
    pub subject: IdentityTarget,
    pub name: String,
    pub account_id: Option<String>,
    pub storage: Reading<u64>,
    pub traffic: Reading<Traffic>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ScopedMeasurement {
    pub scope: RegistryScope,
    pub measurement: Measurement,
}

#[derive(Default, Deserialize, Serialize)]
pub struct Document {
    pub settings: Settings,
    pub measurements: Vec<ScopedMeasurement>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub settings: Settings,
    pub endpoints: Vec<Endpoint>,
    pub measurements: Vec<Measurement>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub currency: String,
    pub rates: Option<Rates>,
    pub measurements: Vec<Measurement>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionResult {
    pub day: NaiveDate,
    pub subjects: usize,
    pub storage_records: usize,
    pub traffic_records: usize,
    pub failures: Vec<Failure>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Endpoint,
    Storage,
    Traffic,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub endpoint_id: Uuid,
    pub subject: Option<IdentityTarget>,
    pub source: Source,
    pub detail: String,
}
