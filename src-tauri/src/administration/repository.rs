use std::collections::BTreeMap;

use reqwest::{Method, StatusCode};
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
    let method = operation
        .method
        .as_deref()
        .unwrap_or("GET")
        .parse::<Method>()
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    rgw_request(
        session,
        method,
        &operation.action,
        operation.fixed.iter().chain(values),
    )
    .await
}

pub(crate) async fn rgw_request(
    session: &Session,
    method: Method,
    resource: &str,
    filters: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
) -> Result<String> {
    rgw_request_with_body(session, method, resource, filters, None).await
}

pub(crate) async fn rgw_request_with_body(
    session: &Session,
    method: Method,
    resource: &str,
    filters: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
    json: Option<Vec<u8>>,
) -> Result<String> {
    let request = rgw_document(session, method, resource, filters, json)?;
    SignedClient::for_connection(&session.profile)?
        .send(session, "s3", request)
        .await
}

pub(crate) struct RgwClient<'a> {
    session: &'a Session,
    client: SignedClient,
}

impl<'a> RgwClient<'a> {
    pub(crate) fn new(session: &'a Session) -> Result<Self> {
        Ok(Self {
            session,
            client: SignedClient::for_connection(&session.profile)?,
        })
    }

    pub(crate) async fn response(
        &self,
        method: Method,
        resource: &str,
        filters: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
    ) -> Result<(StatusCode, String)> {
        let request = rgw_document(self.session, method, resource, filters, None)?;
        self.client.send_document(self.session, "s3", request).await
    }
}

fn rgw_document(
    session: &Session,
    method: Method,
    resource: &str,
    filters: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
    json: Option<Vec<u8>>,
) -> Result<http::Request<Vec<u8>>> {
    let mut url = parse_endpoint(&session.profile.endpoint)?;
    url.path_segments_mut()
        .map_err(|_| AppError::Invalid("Invalid Ceph endpoint.".into()))?
        .pop_if_empty()
        .push("admin")
        .extend(resource.split('/'));
    url.query_pairs_mut()
        .append_pair("format", "json")
        .extend_pairs(filters);
    let content_type = json.as_ref().map(|_| "application/json");
    request(method, url, content_type, json.unwrap_or_default())
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
        context: session.context.clone(),
        profile: crate::connection::model::Connection {
            region,
            ..session.profile.clone()
        },
        credentials: session.credentials.clone(),
        s3: session.s3.clone(),
        openstack: session.openstack.clone(),
    };
    let request = request(
        Method::POST,
        url,
        Some("application/x-www-form-urlencoded; charset=utf-8"),
        body,
    )?;
    SignedClient::for_connection(&session.profile)?
        .send(&adjusted, service, request)
        .await
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
