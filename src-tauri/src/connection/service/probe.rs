use reqwest::Method;
use url::Url;

use crate::{
    bucket::service::BucketService,
    common::{
        error::{AppError, Result},
        signing::{SignedClient, request},
    },
    connection::model::{CapabilityState, IdentityKind},
};

use super::Session;

pub async fn check(session: &Session, identity: IdentityKind) -> (CapabilityState, Option<usize>) {
    let result = match identity {
        IdentityKind::S3 | IdentityKind::Runtime => BucketService::list(session)
            .await
            .map(|buckets| Some(buckets.len())),
        _ => check_admin(session, identity).await.map(|_| None),
    };
    match result {
        Ok(count) => (CapabilityState::Detected, count),
        Err(error) => (classify(&error), None),
    }
}

async fn check_admin(session: &Session, identity: IdentityKind) -> Result<()> {
    let (resource, filters): (&str, &[(&str, &str)]) = match identity {
        IdentityKind::AdminOps => ("metadata/user", &[("max-entries", "1")]),
        IdentityKind::Supervision => ("usage", &[("show-entries", "false")]),
        IdentityKind::CephAdmin => ("bucket", &[("stats", "false"), ("max-entries", "1")]),
        _ => {
            return Err(AppError::Invalid(
                "Select an administrative identity.".into(),
            ));
        }
    };
    let mut url = Url::parse(&session.profile.endpoint)
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    url.path_segments_mut()
        .map_err(|_| AppError::Invalid("Invalid service endpoint.".into()))?
        .pop_if_empty()
        .push("admin")
        .extend(resource.split('/'));
    url.query_pairs_mut()
        .append_pair("format", "json")
        .extend_pairs(filters);
    let response = SignedClient::for_connection(&session.profile)?
        .send(session, "s3", request(Method::GET, url, None, Vec::new())?)
        .await?;
    serde_json::from_str::<serde_json::Value>(&response).map_err(|_| {
        AppError::Storage("The endpoint did not return a Ceph Admin Ops response.".into())
    })?;
    Ok(())
}

fn classify(error: &AppError) -> CapabilityState {
    let message = error.to_string();
    let code = message.split([' ', ':']).next().unwrap_or_default();
    match code {
        "401"
        | "403"
        | "AccessDenied"
        | "Forbidden"
        | "Unauthorized"
        | "InvalidAccessKeyId"
        | "SignatureDoesNotMatch"
        | "ExpiredToken"
        | "InvalidToken" => CapabilityState::Denied,
        "404" | "405" | "501" | "NotImplemented" | "MethodNotAllowed" => {
            CapabilityState::Unavailable
        }
        _ => CapabilityState::Unknown,
    }
}
