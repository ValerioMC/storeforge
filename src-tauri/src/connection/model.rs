use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Aws,
    Ceph,
    Compatible,
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
}

#[derive(Clone, Deserialize, Serialize)]
pub struct StoredConnection {
    pub profile: Connection,
    pub secret_id: Uuid,
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
pub struct ConnectionDraft {
    pub id: Option<Uuid>,
    pub name: String,
    pub provider: Provider,
    pub endpoint: String,
    pub region: String,
    pub path_style: bool,
    pub known_buckets: Vec<String>,
    pub credentials: Option<Secret>,
}
