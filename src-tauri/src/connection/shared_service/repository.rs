use std::{path::PathBuf, time::Duration};

use reqwest::{Method, RequestBuilder, Response, StatusCode, header};
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

pub(crate) struct SharedServiceRepository {
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
        path: &str,
        query: &Q,
        token: Option<&str>,
    ) -> Result<T> {
        let request = self
            .request(Method::GET, origin, path, token)?
            .header(header::ACCEPT, "application/json")
            .query(query);
        Self::json(request).await
    }

    pub async fn change<T: DeserializeOwned, D: Serialize>(
        &self,
        method: Method,
        origin: &Url,
        path: &str,
        token: &str,
        draft: &D,
    ) -> Result<T> {
        Self::json(self.change_request(method, origin, path, token, draft)?).await
    }

    pub async fn long_change<T: DeserializeOwned, D: Serialize>(
        &self,
        method: Method,
        origin: &Url,
        path: &str,
        token: &str,
        draft: &D,
    ) -> Result<T> {
        let request = self.change_request(method, origin, path, token, draft)?;
        Self::json(request.timeout(Duration::from_secs(600))).await
    }

    fn change_request<D: Serialize>(
        &self,
        method: Method,
        origin: &Url,
        path: &str,
        token: &str,
        draft: &D,
    ) -> Result<RequestBuilder> {
        Ok(self
            .request(method, origin, path, Some(token))?
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(draft)?))
    }

    pub async fn csv<Q: Serialize>(
        &self,
        origin: &Url,
        path: &str,
        query: &Q,
        token: &str,
    ) -> Result<String> {
        let request = self
            .request(Method::GET, origin, path, Some(token))?
            .header(header::ACCEPT, "text/csv")
            .query(query);
        let response = Self::send(request).await?;
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if content_type.split(';').next() != Some("text/csv") {
            return Err(AppError::Storage(
                "The shared service did not return a CSV document.".into(),
            ));
        }
        response
            .text()
            .await
            .map_err(|_| AppError::Storage("Could not read the access review CSV.".into()))
    }

    fn request(
        &self,
        method: Method,
        origin: &Url,
        path: &str,
        token: Option<&str>,
    ) -> Result<RequestBuilder> {
        let url = origin
            .join(path)
            .map_err(|_| AppError::Invalid("Invalid service route.".into()))?;
        if url.origin() != origin.origin() {
            return Err(AppError::Invalid(
                "The request must use the configured service origin.".into(),
            ));
        }
        let request = self.client.request(method, url);
        Ok(match token {
            Some(token) => request.bearer_auth(token),
            None => request,
        })
    }

    async fn send(request: RequestBuilder) -> Result<Response> {
        let response = request
            .send()
            .await
            .map_err(|_| AppError::Storage("Could not reach the shared service.".into()))?;
        check_response(response)
    }

    async fn json<T: DeserializeOwned>(request: RequestBuilder) -> Result<T> {
        let body = read_body(Self::send(request).await?).await?;
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
