use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    bucket::workbench::BucketTarget,
    common::error::{AppError, Result},
};

use super::{super::repository::RgwClient, model::Outcome};

#[derive(Deserialize)]
pub(super) struct BucketBinding {
    pub bucket: String,
    #[serde(default)]
    pub tenant: String,
    #[serde(alias = "bucket_id", alias = "bucket-id")]
    pub id: String,
    pub owner: Option<String>,
}

pub(super) type Fields = Vec<(&'static str, String)>;

pub(super) fn bucket_label(target: &BucketTarget) -> String {
    if target.tenant.is_empty() {
        target.name.clone()
    } else {
        format!("{}/{}", target.tenant, target.name)
    }
}

pub(super) async fn bucket_binding(
    client: &RgwClient<'_>,
    target: &BucketTarget,
) -> Result<BucketBinding> {
    let qualified = bucket_label(target);
    let (status, body) = client
        .response(
            Method::GET,
            "bucket",
            [("bucket", qualified.as_str()), ("stats", "true")],
        )
        .await?;
    if !status.is_success() {
        let code = provider_result(status, &body)
            .code
            .unwrap_or_else(|| "RGW error".into());
        return Err(AppError::Storage(format!(
            "Bucket verification: HTTP {status}, {code}."
        )));
    }
    serde_json::from_str(&body)
        .map_err(|_| AppError::Storage("RGW did not return a verifiable bucket binding.".into()))
}

pub(super) async fn call(
    client: &RgwClient<'_>,
    method: Method,
    resource: &str,
    fields: Fields,
) -> Result<Outcome> {
    let (status, body) = client.response(method, resource, fields).await?;
    Ok(provider_result(status, &body))
}

pub(super) fn provider_result(status: StatusCode, body: &str) -> Outcome {
    let document = serde_json::from_str::<Value>(body).ok();
    let success = status.is_success();
    let code = (!success)
        .then(|| document.as_ref().and_then(error_code))
        .flatten();
    let details = document.as_ref().and_then(diagnostic_fields);
    let message = if success {
        "RGW completed the request."
    } else {
        "RGW declined the request."
    };
    Outcome {
        success,
        status: Some(status.as_u16()),
        code,
        message: message.into(),
        details,
    }
}

pub(super) fn rejected(error: AppError) -> Outcome {
    Outcome {
        success: false,
        status: None,
        code: None,
        message: error.to_string(),
        details: None,
    }
}

fn error_code(document: &Value) -> Option<String> {
    let document = document.get("Error").unwrap_or(document);
    document
        .get("Code")
        .or_else(|| document.get("code"))
        .and_then(Value::as_str)
        .filter(|code| {
            !code.is_empty()
                && code.len() <= 80
                && code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .map(str::to_owned)
}

fn diagnostic_fields(document: &Value) -> Option<Value> {
    let fields: Map<String, Value> = document
        .as_object()?
        .iter()
        .filter_map(|(name, value)| {
            let field = match name.as_str() {
                "check_result" | "existing_header" | "calculated_header" | "usage" | "rgw.main"
                | "rgw.multimeta" | "rgw.cloudtiered" => diagnostic_fields(value),
                "num_entries"
                | "num_objects"
                | "size"
                | "size_actual"
                | "size_utilized"
                | "size_kb"
                | "size_kb_actual"
                | "num_shards"
                | "total_size"
                | "total_size_rounded"
                | "total_size_utilized" => value.is_number().then(|| value.clone()),
                "invalid_multipart_entries" => {
                    value.as_array().map(|items| Value::from(items.len()))
                }
                _ => None,
            }?;
            Some((name.clone(), field))
        })
        .collect();
    (!fields.is_empty()).then_some(Value::Object(fields))
}
