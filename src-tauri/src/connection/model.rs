use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Aws,
    Ceph,
    Compatible,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IdentityKind {
    S3,
    AdminOps,
    Runtime,
    Supervision,
    CephAdmin,
}

impl IdentityKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::S3 => "S3",
            Self::AdminOps => "Admin Ops",
            Self::Runtime => "Runtime",
            Self::Supervision => "Supervision",
            Self::CephAdmin => "Ceph Admin",
        }
    }

    pub fn requires_ceph(self) -> bool {
        matches!(self, Self::AdminOps | Self::Supervision | Self::CephAdmin)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityState {
    #[default]
    Unknown,
    Detected,
    Unavailable,
    Denied,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Workspace {
    Browser,
    Manager,
    CephAdmin,
    StorageOps,
    Admin,
    Portal,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct EndpointCoordinates {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EndpointSettings {
    pub tenant: String,
    pub account: String,
    pub realm: String,
    pub default_endpoint: bool,
    pub standard_tags: Vec<String>,
    pub administrative_tags: Vec<String>,
    pub ca_certificate: String,
    pub browser_enabled: bool,
    pub manager_enabled: bool,
    pub default_workspace: Option<Workspace>,
    pub coordinates: Option<EndpointCoordinates>,
}

impl Default for EndpointSettings {
    fn default() -> Self {
        Self {
            tenant: String::new(),
            account: String::new(),
            realm: String::new(),
            default_endpoint: false,
            standard_tags: Vec::new(),
            administrative_tags: Vec::new(),
            ca_certificate: String::new(),
            browser_enabled: true,
            manager_enabled: true,
            default_workspace: None,
            coordinates: None,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceIdentity {
    pub principal_id: Uuid,
    pub endpoint: String,
    pub enabled: bool,
    pub managed_externally: bool,
    pub capability: CapabilityState,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: Uuid,
    pub name: String,
    pub provider: Provider,
    pub endpoint: String,
    pub region: String,
    pub path_style: bool,
    pub known_buckets: Vec<String>,
    #[serde(default)]
    pub endpoint_id: Option<Uuid>,
    #[serde(default)]
    pub revision: Option<Uuid>,
    #[serde(default)]
    pub settings: EndpointSettings,
    #[serde(default)]
    pub s3_capability: CapabilityState,
    #[serde(default)]
    pub services: BTreeMap<IdentityKind, ServiceIdentity>,
}

impl Connection {
    pub fn endpoint_id(&self) -> Uuid {
        self.endpoint_id.unwrap_or(self.id)
    }

    pub fn principal_id(&self, identity: IdentityKind) -> Option<Uuid> {
        match identity {
            IdentityKind::S3 => Some(self.id),
            _ => self
                .services
                .get(&identity)
                .map(|service| service.principal_id),
        }
    }

    pub fn capability(&self, identity: IdentityKind) -> CapabilityState {
        if identity.requires_ceph() && self.provider != Provider::Ceph {
            return CapabilityState::Unavailable;
        }
        match identity {
            IdentityKind::S3 => self.s3_capability,
            _ => self
                .services
                .get(&identity)
                .filter(|service| service.enabled)
                .map_or(CapabilityState::Unavailable, |service| service.capability),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionContext {
    pub connection_id: Uuid,
    pub endpoint_id: Uuid,
    pub principal_id: Uuid,
    pub identity: IdentityKind,
    pub tenant: String,
    pub account: String,
    pub region: String,
    pub workspace: Workspace,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct StoredConnection {
    pub profile: Connection,
    pub secret_id: Uuid,
    #[serde(default)]
    pub service_secret_ids: BTreeMap<IdentityKind, Uuid>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Secret {
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDraft {
    pub endpoint: String,
    pub enabled: bool,
    pub credentials: Option<Secret>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDraft {
    pub id: Option<Uuid>,
    pub name: String,
    pub provider: Provider,
    pub endpoint: String,
    pub region: String,
    pub path_style: bool,
    pub known_buckets: Vec<String>,
    pub credentials: Option<Secret>,
    pub settings: Option<EndpointSettings>,
    #[serde(default)]
    pub validate_s3: bool,
    #[serde(default)]
    pub reconcile: BTreeMap<IdentityKind, ServiceDraft>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionCheck {
    pub bucket_count: Option<usize>,
    pub capabilities: BTreeMap<IdentityKind, CapabilityState>,
}
