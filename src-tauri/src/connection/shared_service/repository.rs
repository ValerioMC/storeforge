use std::{path::PathBuf, time::Duration};

use reqwest::{Response, StatusCode, header};
use serde::{Serialize, de::DeserializeOwned};
use url::Url;

use crate::common::{
    document::{read_document, write_document},
    error::{AppError, Result},
};

use super::model::ServiceProfile;

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct ProfileRepository {
    directory: PathBuf,
}

impl ProfileRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn list(&self) -> Result<Vec<ServiceProfile>> {
        read_document(&self.directory.join("shared-services.json"))
    }

    pub fn write(&self, profiles: &[&ServiceProfile]) -> Result<()> {
        write_document(
            &self.directory,
            &self.directory.join("shared-services.json"),
            profiles,
        )
    }
}

pub(super) struct SharedServiceRepository {
    client: reqwest::Client,
}

impl SharedServiceRepository {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| AppError::Storage("Could not initialize shared service access.".into()))?;
        Ok(Self { client })
    }

    pub async fn get<T: DeserializeOwned, Q: Serialize + ?Sized>(
        &self,
        origin: &Url,
        path: &'static str,
        query: &Q,
        token: Option<&str>,
    ) -> Result<T> {
        let url = origin
            .join(path)
            .map_err(|_| AppError::Invalid("Invalid service route.".into()))?;
        if url.origin() != origin.origin() {
            return Err(AppError::Invalid(
                "The request must use the configured service origin.".into(),
            ));
        }
        let mut request = self
            .client
            .get(url)
            .header(header::ACCEPT, "application/json")
            .query(query);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .map_err(|_| AppError::Storage("Could not reach the shared service.".into()))?;
        let response = check_response(response)?;
        let body = read_body(response).await?;
        serde_json::from_slice(&body).map_err(|_| {
            AppError::Storage("The shared service returned an unexpected response.".into())
        })
    }
}

fn check_response(response: Response) -> Result<Response> {
    match response.status() {
        StatusCode::UNAUTHORIZED => Err(AppError::Reauthenticate(
            "Authorize the shared service again.".into(),
        )),
        StatusCode::FORBIDDEN => Err(AppError::AuthorizationDenied(
            "The shared service denied this request. Check the token scopes and workspace access."
                .into(),
        )),
        StatusCode::NOT_FOUND => Err(AppError::Missing(
            "This action is unavailable on the shared service.".into(),
        )),
        status if status.is_redirection() => Err(AppError::Storage(
            "The shared service redirected the request. Check the configured origin.".into(),
        )),
        status if !status.is_success() => Err(AppError::Storage(format!(
            "The shared service returned HTTP {}.",
            status.as_u16()
        ))),
        _ => Ok(response),
    }
}

async fn read_body(mut response: Response) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppError::Storage("Could not read the shared service response.".into()))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(AppError::Storage(
                "The shared service response is too large.".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
