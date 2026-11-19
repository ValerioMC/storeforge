use std::time::{Duration, SystemTime};

use aws_sigv4::http_request::{
    PayloadChecksumKind, PercentEncodingMode, SignableBody, SignableRequest, SigningSettings,
    UriPathNormalizationMode, sign,
};
use aws_sigv4::sign::v4;
use reqwest::{Method, StatusCode, header};
use serde::Deserialize;
use url::Url;

use crate::connection::{
    model::Connection,
    service::{Session, normalize_proxy_url},
};

use super::error::{AppError, Result, is_unsupported_code};

pub struct SignedClient(reqwest::Client);

impl SignedClient {
    pub fn for_connection(profile: &Connection) -> Result<Self> {
        Ok(Self(endpoint_client(profile, Duration::from_secs(45))?))
    }
}

pub fn endpoint_client(profile: &Connection, timeout: Duration) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout);
    let proxy = normalize_proxy_url(&profile.settings.proxy_url)?;
    if !proxy.is_empty() {
        builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(transport_error)?);
    }
    if !profile.settings.ca_certificate.trim().is_empty() {
        let certificates =
            reqwest::Certificate::from_pem_bundle(profile.settings.ca_certificate.as_bytes())
                .map_err(transport_error)?;
        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
    }
    builder.build().map_err(transport_error)
}

impl SignedClient {
    pub async fn send(
        &self,
        session: &Session,
        service: &str,
        request: http::Request<Vec<u8>>,
    ) -> Result<String> {
        let (status, text) = self.send_document(session, service, request).await?;
        if status.is_success() {
            return Ok(text);
        }
        Err(response_error(status, &text))
    }

    pub(crate) async fn send_document(
        &self,
        session: &Session,
        service: &str,
        request: http::Request<Vec<u8>>,
    ) -> Result<(StatusCode, String)> {
        session.validate().await?;
        let request = sign_request(session, service, request)?;
        let (parts, body) = request.into_parts();
        let response = self
            .0
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(body)
            .send()
            .await
            .map_err(transport_error)?;
        read_response(response).await
    }
}

pub fn request(
    method: Method,
    url: Url,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> Result<http::Request<Vec<u8>>> {
    let mut builder = http::Request::builder().method(method).uri(url.as_str());
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if content_type == Some("application/xml") {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use md5::{Digest, Md5};
        builder = builder.header("content-md5", STANDARD.encode(Md5::digest(&body)));
    }
    builder
        .body(body)
        .map_err(|error| AppError::Invalid(error.to_string()))
}

fn sign_request(
    session: &Session,
    service: &str,
    mut request: http::Request<Vec<u8>>,
) -> Result<http::Request<Vec<u8>>> {
    let identity = session.credentials.clone().into();
    let mut settings = SigningSettings::default();
    if service == "s3" {
        settings.payload_checksum_kind = PayloadChecksumKind::XAmzSha256;
        settings.percent_encoding_mode = PercentEncodingMode::Single;
        settings.uri_path_normalization_mode = UriPathNormalizationMode::Disabled;
    }
    let params = v4::SigningParams::builder()
        .identity(&identity)
        .region(&session.profile.region)
        .name(service)
        .time(SystemTime::now())
        .settings(settings)
        .build()
        .map_err(|error| AppError::Invalid(error.to_string()))?
        .into();
    let uri = request.uri().to_string();
    let headers = request
        .headers()
        .iter()
        .map(|(name, value)| {
            Ok((
                name.as_str(),
                value
                    .to_str()
                    .map_err(|error| AppError::Invalid(error.to_string()))?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let signable = SignableRequest::new(
        request.method().as_str(),
        &uri,
        headers.into_iter(),
        SignableBody::Bytes(request.body()),
    )
    .map_err(|error| AppError::Invalid(error.to_string()))?;
    let (instructions, _) = sign(signable, &params)
        .map_err(|error| AppError::Invalid(error.to_string()))?
        .into_parts();
    instructions.apply_to_request_http1x(&mut request);
    Ok(request)
}

async fn read_response(mut response: reqwest::Response) -> Result<(StatusCode, String)> {
    const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
    let status = response.status();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if bytes.len().saturating_add(chunk.len()) > MAX_DOCUMENT_BYTES {
            return Err(AppError::Storage(
                "Response exceeded 8 MiB. Narrow the list or query and try again.".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ProviderErrorCode {
    #[serde(alias = "code")]
    code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ProviderErrorDocument {
    #[serde(alias = "code")]
    code: Option<String>,
    error: Option<ProviderErrorCode>,
}

fn response_error(status: StatusCode, text: &str) -> AppError {
    if matches!(
        status,
        StatusCode::NOT_IMPLEMENTED | StatusCode::METHOD_NOT_ALLOWED
    ) || response_error_code(text)
        .as_deref()
        .is_some_and(is_unsupported_code)
    {
        return AppError::unsupported();
    }
    let detail: String = text.chars().take(1200).collect();
    let message = format!("{status} {detail}");
    if status == StatusCode::NOT_FOUND {
        return AppError::Missing(message);
    }
    AppError::Storage(message)
}

fn response_error_code(text: &str) -> Option<String> {
    let document: ProviderErrorDocument = match text.trim_start().chars().next()? {
        '<' => quick_xml::de::from_str(text).ok()?,
        '{' => serde_json::from_str(text).ok()?,
        _ => return None,
    };
    document.code.or_else(|| document.error?.code)
}

fn transport_error(error: reqwest::Error) -> AppError {
    AppError::Storage(error.without_url().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_unsupported_actions_without_provider_documents() {
        for (status, body) in [
            (StatusCode::NOT_IMPLEMENTED, ""),
            (
                StatusCode::METHOD_NOT_ALLOWED,
                "<html>Method not allowed</html>",
            ),
            (
                StatusCode::BAD_REQUEST,
                r#"<?xml version="1.0"?><s3:Error xmlns:s3="urn:s3"><s3:Code>NotSupported</s3:Code><s3:RequestId>private</s3:RequestId></s3:Error>"#,
            ),
            (
                StatusCode::BAD_REQUEST,
                "<ErrorResponse><Error><Code>UnsupportedOperation</Code></Error></ErrorResponse>",
            ),
            (
                StatusCode::NOT_FOUND,
                r#"{"Code":"XNotImplemented","RequestId":"private"}"#,
            ),
        ] {
            assert_eq!(
                serde_json::to_value(response_error(status, body)).unwrap(),
                serde_json::json!({
                    "kind": "unsupported",
                    "message": "This endpoint does not support this action.",
                })
            );
        }
    }

    #[test]
    fn preserves_missing_configurations_and_access_failures() {
        let missing = "<Error><Code>NoSuchTagSet</Code></Error>";
        let denied = "<Error><Code>AccessDenied</Code><Message>Unable to check NotSupported.</Message></Error>";
        assert!(matches!(
            response_error(StatusCode::NOT_FOUND, missing),
            AppError::Missing(message) if message.contains(missing)
        ));
        assert!(matches!(
            response_error(StatusCode::FORBIDDEN, denied),
            AppError::Storage(message) if message.contains(denied)
        ));
    }
}
