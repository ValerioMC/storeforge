use std::collections::BTreeMap;

use reqwest::Method;
use url::Url;

use crate::{
    common::{
        error::{AppError, Result},
        signing::{SignedClient, request},
    },
    connection::{model::Provider, service::Session},
};

use super::model::{Operation, Protocol};

pub async fn execute(
    session: &Session,
    operation: &Operation,
    values: &BTreeMap<String, String>,
) -> Result<String> {
    if operation.protocol == Protocol::Rgw {
        return rgw(session, operation, values).await;
    }
    query(session, operation, values).await
}

async fn rgw(
    session: &Session,
    operation: &Operation,
    values: &BTreeMap<String, String>,
) -> Result<String> {
    let mut url = parse_endpoint(&session.profile.endpoint)?;
    url.path_segments_mut()
        .map_err(|_| AppError::Invalid("Invalid Ceph endpoint.".into()))?
        .pop_if_empty()
        .push("admin")
        .extend(operation.action.split('/'));
    url.query_pairs_mut()
        .append_pair("format", "json")
        .extend_pairs(&operation.fixed)
        .extend_pairs(values);
    let method = operation
        .method
        .as_deref()
        .unwrap_or("GET")
        .parse::<Method>()
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    SignedClient::new()?
        .send(session, "s3", request(method, url, None, Vec::new())?)
        .await
}

async fn query(
    session: &Session,
    operation: &Operation,
    values: &BTreeMap<String, String>,
) -> Result<String> {
    let (service, version) = if operation.protocol == Protocol::Iam {
        ("iam", "2010-05-08")
    } else {
        ("sns", "2010-03-31")
    };
    let (endpoint, region) = endpoint_for_query(session, operation.protocol);
    let url = parse_endpoint(&endpoint)?;
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("Action", &operation.action)
            .append_pair("Version", version)
            .extend_pairs(values);
        form.finish().into_bytes()
    };
    let adjusted = Session {
        profile: crate::connection::model::Connection {
            region,
            ..session.profile.clone()
        },
        credentials: session.credentials.clone(),
        s3: session.s3.clone(),
    };
    let request = request(
        Method::POST,
        url,
        Some("application/x-www-form-urlencoded; charset=utf-8"),
        body,
    )?;
    SignedClient::new()?.send(&adjusted, service, request).await
}

fn endpoint_for_query(session: &Session, protocol: Protocol) -> (String, String) {
    if session.profile.provider != Provider::Aws && !session.profile.endpoint.is_empty() {
        return (
            session.profile.endpoint.clone(),
            session.profile.region.clone(),
        );
    }
    let region = &session.profile.region;
    let suffix = if region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    if protocol == Protocol::Iam {
        if region.starts_with("cn-") {
            return (format!("https://iam.{suffix}"), "cn-north-1".into());
        }
        if region.starts_with("us-gov-") {
            return (
                "https://iam.us-gov.amazonaws.com".into(),
                "us-gov-west-1".into(),
            );
        }
        return (format!("https://iam.{suffix}"), "us-east-1".into());
    }
    (format!("https://sns.{region}.{suffix}"), region.clone())
}

fn parse_endpoint(endpoint: &str) -> Result<Url> {
    Url::parse(endpoint).map_err(|error| AppError::Invalid(error.to_string()))
}
