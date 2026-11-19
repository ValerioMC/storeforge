use reqwest::Method;
use url::Url;

use crate::{
    common::{
        error::{AppError, Result},
        signing::{SignedClient, request},
    },
    connection::service::Session,
};

use super::model::{Configuration, ConfigurationChange};

pub async fn configure(
    session: &Session,
    bucket: &str,
    configuration: Configuration,
    change: ConfigurationChange,
) -> Result<String> {
    let (subresource, content_type, removable) = resource(configuration);
    let url = configuration_url(session, bucket, subresource)?;
    let (method, content_type, body) = match change {
        ConfigurationChange::Read => (Method::GET, None, Vec::new()),
        ConfigurationChange::Write(document) if !document.trim().is_empty() => {
            (Method::PUT, Some(content_type), document.into_bytes())
        }
        ConfigurationChange::Write(_) => {
            return Err(AppError::Invalid("Enter a configuration document.".into()));
        }
        ConfigurationChange::Remove if removable => (Method::DELETE, None, Vec::new()),
        ConfigurationChange::Remove => {
            return Err(AppError::Invalid(
                "This configuration uses an update to reset its settings.".into(),
            ));
        }
    };
    let response = SignedClient::for_connection(&session.profile)?
        .send(session, "s3", request(method, url, content_type, body)?)
        .await;
    match response {
        Err(AppError::Missing(message)) if !is_missing_configuration(&message) => {
            Err(AppError::Storage(message))
        }
        response => response,
    }
}

fn is_missing_configuration(message: &str) -> bool {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct S3Error {
        code: String,
    }

    let document = message.find('<').map(|start| &message[start..]);
    if let Some(document) = document
        && let Ok(error) = quick_xml::de::from_str::<S3Error>(document)
    {
        return (error.code.starts_with("NoSuch") && error.code != "NoSuchBucket")
            || error.code == "ObjectLockConfigurationNotFoundError";
    }
    message.trim() == "404 Not Found"
}

pub(crate) fn configuration_removable(configuration: Configuration) -> bool {
    let (_, _, removable) = resource(configuration);
    removable
}

fn resource(configuration: Configuration) -> (&'static str, &'static str, bool) {
    use Configuration::*;
    let (subresource, removable) = match configuration {
        Versioning => ("versioning", false),
        Lifecycle => ("lifecycle", true),
        Cors => ("cors", true),
        Policy => return ("policy", "application/json", true),
        Acl => ("acl", false),
        OwnershipControls => ("ownershipControls", true),
        PublicAccessBlock => ("publicAccessBlock", true),
        Encryption => ("encryption", true),
        ObjectLock => ("object-lock", false),
        Tagging => ("tagging", true),
        Logging => ("logging", false),
        Notification => ("notification", false),
        Replication => ("replication", true),
        Website => ("website", true),
        RequestPayment => ("requestPayment", false),
    };
    (subresource, "application/xml", removable)
}

pub(crate) fn configuration_url(session: &Session, bucket: &str, subresource: &str) -> Result<Url> {
    let endpoint = if session.profile.endpoint.is_empty() {
        let suffix = if session.profile.region.starts_with("cn-") {
            "amazonaws.com.cn"
        } else {
            "amazonaws.com"
        };
        format!("https://s3.{}.{suffix}", session.profile.region)
    } else {
        session.profile.endpoint.clone()
    };
    let mut url = Url::parse(&endpoint).map_err(|error| AppError::Invalid(error.to_string()))?;
    if session.profile.path_style || bucket.contains('.') {
        url.path_segments_mut()
            .map_err(|_| AppError::Invalid("Invalid S3 endpoint.".into()))?
            .pop_if_empty()
            .push(bucket);
    } else {
        let host = format!(
            "{bucket}.{}",
            url.host_str()
                .ok_or_else(|| AppError::Invalid("Invalid S3 endpoint.".into()))?
        );
        url.set_host(Some(&host))
            .map_err(|error| AppError::Invalid(error.to_string()))?;
    }
    url.set_query(Some(subresource));
    Ok(url)
}
