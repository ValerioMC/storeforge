use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Iam,
    Sns,
    Rgw,
}

#[derive(Deserialize)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub required: bool,
}

#[derive(Deserialize)]
pub struct Operation {
    pub id: String,
    pub label: String,
    pub protocol: Protocol,
    pub action: String,
    pub method: Option<String>,
    #[serde(default)]
    pub fixed: BTreeMap<String, String>,
    pub fields: Vec<Field>,
    pub write: bool,
}

#[derive(Deserialize)]
pub struct AdminRequest {
    pub operation: String,
    pub values: BTreeMap<String, String>,
    pub confirmed: bool,
}
