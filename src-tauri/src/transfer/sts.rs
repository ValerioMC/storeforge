use std::collections::BTreeMap;

use aws_sdk_s3::config::Credentials;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    common::{
        error::{AppError, Result},
        signing::{SignedClient, request},
    },
    connection::{model::Provider, service::Session},
    object::model::ObjectId,
};

use super::security::{UploadEncryption, credentials_expiry};

#[derive(Clone, Copy)]
pub enum RoleScope<'a> {
    Read(&'a [ObjectId]),
    Write { bucket: &'a str, keys: &'a [String] },
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SessionPolicy {
    version: &'static str,
    statement: Vec<Statement>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Statement {
    effect: &'static str,
    action: Vec<&'static str>,
    resource: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition: Option<Condition>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Condition {
    string_equals: BTreeMap<&'static str, Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AssumeRoleResponse {
    assume_role_result: AssumeRoleResult,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AssumeRoleResult {
    credentials: IssuedCredentials,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IssuedCredentials {
    access_key_id: String,
    secret_access_key: String,
    session_token: String,
    expiration: String,
}

pub async fn assume(
    session: &Session,
    role: &str,
    scope: RoleScope<'_>,
    encryption: Option<&UploadEncryption>,
) -> Result<Credentials> {
    let partition = validate_role(role)?;
    let endpoint = sts_endpoint(session, partition)?;
    let via_s3 = (session.profile.provider == Provider::Aws)
        .then(|| s3_service(partition, &session.profile.region));
    let policy = serialize_policy(partition, scope, encryption, via_s3.as_deref())?;
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("Action", "AssumeRole")
            .append_pair("Version", "2011-06-15")
            .append_pair("RoleArn", role)
            .append_pair(
                "RoleSessionName",
                &format!("storeforge-{}", Uuid::new_v4().simple()),
            )
            .append_pair("Policy", &policy)
            .finish()
            .into_bytes()
    };
    let request = request(
        Method::POST,
        endpoint,
        Some("application/x-www-form-urlencoded; charset=utf-8"),
        body,
    )?;
    let response = SignedClient::for_connection(&session.profile)?
        .send(session, "sts", request)
        .await
        .map_err(|_| {
            AppError::Credential(
                "STS refused the scoped role session. Check role trust and AssumeRole permissions."
                    .into(),
            )
        })?;
    parse_credentials(Zeroizing::new(response))
}

fn validate_role(role: &str) -> Result<&str> {
    let mut fields = role.splitn(6, ':');
    let invalid = || AppError::Invalid("Enter an IAM role ARN for the temporary session.".into());
    if fields.next() != Some("arn") {
        return Err(invalid());
    }
    let partition = fields
        .next()
        .filter(|value| matches!(*value, "aws" | "aws-cn" | "aws-us-gov"))
        .ok_or_else(invalid)?;
    if fields.next() != Some("iam")
        || fields.next() != Some("")
        || fields.next().is_none()
        || !fields.next().is_some_and(|value| {
            value.starts_with("role/") && value.len() > 5 && !value.contains(char::is_whitespace)
        })
    {
        return Err(invalid());
    }
    Ok(partition)
}

fn sts_endpoint(session: &Session, partition: &str) -> Result<Url> {
    const INVALID: &str = "The connection needs a valid HTTPS STS endpoint.";
    let endpoint = if session.profile.provider == Provider::Aws {
        let region = &session.profile.region;
        let (required_partition, suffix) = if region.starts_with("cn-") {
            ("aws-cn", "amazonaws.com.cn")
        } else if region.starts_with("us-gov-") {
            ("aws-us-gov", "amazonaws.com")
        } else {
            ("aws", "amazonaws.com")
        };
        if partition != required_partition
            || region.is_empty()
            || !region
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(AppError::Invalid(
                "The role ARN partition must match the connection region.".into(),
            ));
        }
        format!("https://sts.{region}.{suffix}")
    } else {
        session.profile.endpoint.clone()
    };
    let url = Url::parse(&endpoint).map_err(|_| AppError::Invalid(INVALID.into()))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
    {
        return Err(AppError::Invalid(INVALID.into()));
    }
    Ok(url)
}

fn s3_service(partition: &str, region: &str) -> String {
    let suffix = if partition == "aws-cn" {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    format!("s3.{region}.{suffix}")
}

fn serialize_policy(
    partition: &str,
    scope: RoleScope<'_>,
    encryption: Option<&UploadEncryption>,
    via_s3: Option<&str>,
) -> Result<String> {
    let mut statements = match scope {
        RoleScope::Read(objects) => read_statements(partition, objects)?,
        RoleScope::Write { bucket, keys } => {
            let actions = vec!["s3:PutObject", "s3:AbortMultipartUpload"];
            statement(
                actions,
                keys.iter()
                    .map(|key| resource(partition, bucket, key))
                    .collect::<Result<_>>()?,
            )
            .into_iter()
            .collect()
        }
    };
    if statements.is_empty() {
        return Err(AppError::Invalid(
            "Select at least one object for the temporary session.".into(),
        ));
    }
    if let Some(statement) = kms_statement(partition, scope, encryption, via_s3)? {
        statements.push(statement);
    }
    let policy = serde_json::to_string(&SessionPolicy {
        version: "2012-10-17",
        statement: statements,
    })?;
    if policy.chars().count() > 2048 {
        return Err(AppError::Invalid(
            "This selection exceeds the STS policy size. Transfer fewer objects per session."
                .into(),
        ));
    }
    Ok(policy)
}

fn read_statements(partition: &str, objects: &[ObjectId]) -> Result<Vec<Statement>> {
    let mut current = Vec::new();
    let mut historical = BTreeMap::<&str, Vec<String>>::new();
    for object in objects {
        let arn = resource(partition, &object.bucket, &object.key)?;
        if let Some(version) = object.version_id.as_deref() {
            if version.contains("${") {
                return Err(AppError::Invalid(
                    "The version ID cannot be represented as an exact STS condition.".into(),
                ));
            }
            historical.entry(version).or_default().push(arn);
        } else {
            current.push(arn);
        }
    }
    let mut statements: Vec<_> = statement(vec!["s3:GetObject"], current)
        .into_iter()
        .collect();
    for (version, resources) in historical {
        if let Some(mut statement) = statement(vec!["s3:GetObjectVersion"], resources) {
            statement.condition = Some(Condition {
                string_equals: BTreeMap::from([("s3:VersionId", vec![version.into()])]),
            });
            statements.push(statement);
        }
    }
    Ok(statements)
}

fn kms_statement(
    partition: &str,
    scope: RoleScope<'_>,
    encryption: Option<&UploadEncryption>,
    via_s3: Option<&str>,
) -> Result<Option<Statement>> {
    let key = match encryption {
        Some(UploadEncryption::S3) => return Ok(None),
        Some(UploadEncryption::Kms { key_id } | UploadEncryption::DualLayerKms { key_id }) => {
            key_id
                .as_deref()
                .filter(|key| {
                    key.starts_with(&format!("arn:{partition}:kms:"))
                        && key.contains(":key/")
                        && exact_resource(key)
                })
                .ok_or_else(|| {
                    AppError::Invalid("Scoped KMS uploads require the full KMS key ARN.".into())
                })?
        }
        None if via_s3.is_none() => return Ok(None),
        None => "*",
    };
    let (action, contexts) = match scope {
        RoleScope::Read(objects) => {
            let mut contexts = Vec::with_capacity(objects.len() * 2);
            for object in objects {
                contexts.push(format!("arn:{partition}:s3:::{}", object.bucket));
                contexts.push(resource(partition, &object.bucket, &object.key)?);
            }
            (vec!["kms:Decrypt"], contexts)
        }
        RoleScope::Write { bucket, keys } => {
            let mut contexts = vec![format!("arn:{partition}:s3:::{bucket}")];
            for key in keys {
                contexts.push(resource(partition, bucket, key)?);
            }
            (vec!["kms:GenerateDataKey", "kms:Decrypt"], contexts)
        }
    };
    let condition = via_s3.map(|service| kms_condition(service, contexts));
    Ok(Some(Statement {
        effect: "Allow",
        action,
        resource: vec![key.into()],
        condition,
    }))
}

fn kms_condition(service: &str, mut contexts: Vec<String>) -> Condition {
    contexts.sort();
    contexts.dedup();
    Condition {
        string_equals: BTreeMap::from([
            ("kms:ViaService", vec![service.into()]),
            ("kms:EncryptionContext:aws:s3:arn", contexts),
        ]),
    }
}

fn statement(action: Vec<&'static str>, mut resource: Vec<String>) -> Option<Statement> {
    resource.sort();
    resource.dedup();
    (!resource.is_empty()).then_some(Statement {
        effect: "Allow",
        action,
        resource,
        condition: None,
    })
}

fn exact_resource(value: &str) -> bool {
    !value.contains(['*', '?']) && !value.contains("${") && !value.chars().any(char::is_control)
}

fn resource(partition: &str, bucket: &str, key: &str) -> Result<String> {
    if bucket.is_empty()
        || bucket.contains(['/', ':'])
        || !exact_resource(bucket)
        || !exact_resource(key)
    {
        return Err(AppError::Invalid(
            "A selected key cannot be represented as an exact STS policy resource.".into(),
        ));
    }
    Ok(format!("arn:{partition}:s3:::{bucket}/{key}"))
}

fn parse_credentials(response: Zeroizing<String>) -> Result<Credentials> {
    let response: AssumeRoleResponse = quick_xml::de::from_str(&response)
        .map_err(|_| AppError::Credential("STS returned an invalid credential response.".into()))?;
    let credentials = response.assume_role_result.credentials;
    let access = Zeroizing::new(credentials.access_key_id);
    let secret = Zeroizing::new(credentials.secret_access_key);
    let token = Zeroizing::new(credentials.session_token);
    let expiry = credentials_expiry(&credentials.expiration)?;
    if access.is_empty() || secret.is_empty() || token.is_empty() {
        return Err(AppError::Credential(
            "STS returned an incomplete credential response.".into(),
        ));
    }
    Ok(Credentials::new(
        access.as_str(),
        secret.as_str(),
        Some(token.to_string()),
        Some(expiry),
        "storeforge-sts",
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/transfer/sts.rs"]
mod tests;
