use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cloud {
    pub id: Uuid,
    #[serde(flatten)]
    pub settings: CloudSettings,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSettings {
    pub name: String,
    pub auth_url: String,
    pub default_domain: String,
    pub managed_endpoints_only: bool,
}

#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum Authentication {
    Password {
        username: String,
        domain: String,
        password: String,
    },
    Token {
        token: String,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(rename(deserialize = "domain_id", serialize = "domainId"))]
    pub domain_id: Option<String>,
    #[serde(default = "enabled_by_default", skip_serializing)]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTarget {
    pub id: String,
    pub region: String,
    pub interface: String,
    pub endpoint: String,
}

#[derive(Serialize)]
pub struct RegionalTarget {
    #[serde(flatten)]
    pub catalog: CatalogTarget,
    pub managed: bool,
    pub allowed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAccess {
    pub id: Uuid,
    pub targets: Vec<RegionalTarget>,
    pub access_keys: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activation {
    pub discovery_id: Uuid,
    pub target_id: String,
    pub access_key: String,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenStackProfile {
    pub cloud_id: Uuid,
    pub project_name: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    pub cloud: Cloud,
    pub session: Option<SessionView>,
    pub reconnect_required: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub identity: String,
    pub expires_at: DateTime<Utc>,
    pub projects: Vec<Project>,
}
