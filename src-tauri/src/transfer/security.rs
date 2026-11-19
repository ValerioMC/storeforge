use std::time::SystemTime;

use aws_sdk_s3::{
    Client,
    config::retry::RetryConfig,
    error::{ProvideErrorMetadata, SdkError},
    types::ServerSideEncryption,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use md5::{Digest, Md5};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::{
    common::error::{AppError, Result, storage_error},
    connection::service::Session,
};

use super::sts::{self, RoleScope};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessInput {
    pub customer_key: Option<String>,
    pub role_arn: Option<String>,
    pub encryption: Option<UploadEncryption>,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UploadEncryption {
    S3,
    Kms { key_id: Option<String> },
    DualLayerKms { key_id: Option<String> },
}

impl UploadEncryption {
    pub fn algorithm(&self) -> ServerSideEncryption {
        match self {
            Self::S3 => ServerSideEncryption::Aes256,
            Self::Kms { .. } => ServerSideEncryption::AwsKms,
            Self::DualLayerKms { .. } => ServerSideEncryption::AwsKmsDsse,
        }
    }

    pub fn key_id(&self) -> Option<&str> {
        match self {
            Self::S3 => None,
            Self::Kms { key_id } | Self::DualLayerKms { key_id } => key_id.as_deref(),
        }
    }
}

pub struct CustomerKey {
    value: Zeroizing<String>,
    md5: String,
}

impl CustomerKey {
    fn parse(value: String) -> Result<Self> {
        let value = Zeroizing::new(value);
        let mut bytes = Zeroizing::new([0u8; 32]);
        let length = STANDARD
            .decode_slice(value.trim(), bytes.as_mut())
            .map_err(|_| invalid_key())?;
        if length != bytes.len() {
            return Err(invalid_key());
        }
        let md5 = STANDARD.encode(Md5::digest(bytes.as_ref()));
        Ok(Self {
            value: Zeroizing::new(STANDARD.encode(bytes.as_ref())),
            md5,
        })
    }

    pub fn value(&self) -> &str {
        self.value.as_str()
    }

    pub fn md5(&self) -> &str {
        &self.md5
    }
}

fn invalid_key() -> AppError {
    AppError::Invalid("Enter a base64-encoded 32-byte customer key.".into())
}

macro_rules! customer_headers {
    ($builder:expr, $key:expr) => {
        match $key {
            Some(key) => $builder
                .sse_customer_algorithm("AES256")
                .sse_customer_key(key.value())
                .sse_customer_key_md5(key.md5()),
            None => $builder,
        }
    };
}
pub(crate) use customer_headers;

#[derive(Clone, Copy, Default)]
pub struct ReadAccess<'a> {
    pub customer: Option<&'a CustomerKey>,
    pub sensitive: bool,
}

#[derive(Clone, Copy, Default)]
pub struct UploadAccess<'a> {
    pub customer: Option<&'a CustomerKey>,
    pub encryption: Option<&'a UploadEncryption>,
    pub temporary: bool,
}

impl UploadAccess<'_> {
    pub fn sensitive(&self) -> bool {
        self.customer.is_some() || self.temporary
    }
}

pub struct OperationAccess {
    pub customer: Option<CustomerKey>,
    pub encryption: Option<UploadEncryption>,
    pub role_arn: Option<String>,
}

impl OperationAccess {
    pub fn parse(input: AccessInput, writes: bool) -> Result<Self> {
        let customer = input.customer_key.map(CustomerKey::parse).transpose()?;
        if input.encryption.is_some() && (!writes || customer.is_some()) {
            return Err(AppError::Invalid(
                "Choose one encryption method for the upload.".into(),
            ));
        }
        if input
            .encryption
            .as_ref()
            .and_then(UploadEncryption::key_id)
            .is_some_and(str::is_empty)
        {
            return Err(AppError::Invalid(
                "Enter a KMS key identifier or use the endpoint default.".into(),
            ));
        }
        Ok(Self {
            customer,
            encryption: input.encryption,
            role_arn: input.role_arn,
        })
    }

    pub fn upload_access(&self) -> UploadAccess<'_> {
        UploadAccess {
            customer: self.customer.as_ref(),
            encryption: self.encryption.as_ref(),
            temporary: self.role_arn.is_some(),
        }
    }

    pub fn read_access(&self) -> ReadAccess<'_> {
        ReadAccess {
            customer: self.customer.as_ref(),
            sensitive: self.customer.is_some() || self.role_arn.is_some(),
        }
    }

    pub async fn client(&self, session: &Session, scope: RoleScope<'_>) -> Result<OperationClient> {
        if self.customer.is_some()
            && !session.profile.endpoint.is_empty()
            && !session.profile.endpoint.starts_with("https://")
        {
            return Err(AppError::Invalid(
                "Customer-key operations require an HTTPS endpoint.".into(),
            ));
        }
        let Some(role) = &self.role_arn else {
            return Ok(OperationClient {
                client: session.s3.clone(),
                expiration: None,
            });
        };
        let credentials = sts::assume(session, role, scope, self.encryption.as_ref()).await?;
        let expiration = credentials.expiry().map(DateTime::<Utc>::from);
        Ok(OperationClient {
            client: session.s3_with_credentials(credentials),
            expiration,
        })
    }
}

pub struct OperationClient {
    client: Client,
    expiration: Option<DateTime<Utc>>,
}

impl OperationClient {
    pub fn client(&self) -> Result<&Client> {
        if self.expiration.is_some_and(|expiry| expiry <= Utc::now()) {
            return Err(expired_session());
        }
        Ok(&self.client)
    }
}

pub fn expired_session() -> AppError {
    AppError::Credential(
        "The temporary session expired. Renew the temporary credentials and start a new operation."
            .into(),
    )
}

pub fn write_client(client: &Client) -> Client {
    Client::from_conf(
        client
            .config()
            .to_builder()
            .retry_config(RetryConfig::disabled())
            .build(),
    )
}

pub fn transfer_error<E: ProvideErrorMetadata>(
    error: SdkError<E>,
    sensitive: bool,
    writing: bool,
) -> AppError {
    let code = error
        .as_service_error()
        .and_then(ProvideErrorMetadata::code);
    if matches!(
        code,
        Some("ExpiredToken" | "ExpiredTokenException" | "TokenRefreshRequired")
    ) {
        return expired_session();
    }
    if writing
        && (matches!(
            error,
            SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) | SdkError::ResponseError(_)
        ) || error
            .raw_response()
            .is_some_and(|response| response.status().as_u16() >= 500))
    {
        return AppError::AmbiguousWrite("The write outcome is unknown. Check the destination before starting another operation.".into());
    }
    if sensitive {
        return match code {
            Some("NoSuchKey" | "NoSuchBucket" | "NoSuchVersion" | "NotFound") => {
                AppError::Missing("The selected resource no longer exists.".into())
            }
            Some("PreconditionFailed" | "ConditionalRequestConflict") => AppError::Storage(
                "The object changed or already exists. Refresh and review the key.".into(),
            ),
            Some("AccessDenied") => {
                AppError::Storage("The endpoint denied this object operation.".into())
            }
            _ => AppError::Storage(
                "The endpoint rejected this encrypted or temporary object operation.".into(),
            ),
        };
    }
    storage_error(error)
}

pub fn credentials_expiry(expiration: &str) -> Result<SystemTime> {
    let expiry = expiration
        .parse::<DateTime<Utc>>()
        .map_err(|_| AppError::Credential("STS omitted a valid expiration.".into()))?;
    if expiry <= Utc::now() {
        return Err(expired_session());
    }
    Ok(expiry.into())
}

#[cfg(test)]
#[path = "../../tests/unit/transfer/security.rs"]
mod tests;
