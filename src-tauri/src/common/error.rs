use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use serde::Serialize;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum AppError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Missing(String),
    #[error("{0}")]
    Storage(String),
    #[error("{0}")]
    AmbiguousWrite(String),
    #[error("{0}")]
    Reauthenticate(String),
    #[error("{0}")]
    AuthorizationDenied(String),
    #[error("Local file: {0}")]
    File(String),
    #[error("Credential store: {0}")]
    Credential(String),
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::File(error.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self::File(error.to_string())
    }
}

impl From<keyring::Error> for AppError {
    fn from(error: keyring::Error) -> Self {
        Self::Credential(error.to_string())
    }
}

pub fn storage_error<E: ProvideErrorMetadata, R: std::fmt::Debug>(
    error: SdkError<E, R>,
) -> AppError {
    let metadata = error.as_service_error();
    let code = metadata.and_then(ProvideErrorMetadata::code);
    let message = metadata.and_then(ProvideErrorMetadata::message);
    match code {
        Some("NoSuchKey" | "NoSuchBucket" | "NoSuchVersion" | "NotFound") => AppError::Missing(
            message
                .unwrap_or("The selected resource no longer exists.")
                .into(),
        ),
        Some(code) => AppError::Storage(format!(
            "{code}: {}",
            message.unwrap_or("Request declined.")
        )),
        None => AppError::Storage(error.to_string()),
    }
}
