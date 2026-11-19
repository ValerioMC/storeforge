use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, Utc};
use percent_encoding::percent_decode_str;
use reqwest::{Method, Response, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Url;
use zeroize::Zeroizing;

use crate::common::{
    document::{read_document, write_document},
    error::{AppError, Result},
};

use super::model::{Authentication, CatalogTarget, Cloud, Project};

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct CloudRepository {
    directory: PathBuf,
    path: PathBuf,
}

impl CloudRepository {
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.into(),
            path: directory.join("openstack-clouds.json"),
        }
    }

    pub fn list(&self) -> Result<Vec<Cloud>> {
        read_document(&self.path)
    }

    pub fn write(&self, clouds: &[&Cloud]) -> Result<()> {
        write_document(&self.directory, &self.path, clouds)
    }
}

pub(super) struct KeystoneRepository {
    client: reqwest::Client,
    base: Url,
}

pub(super) struct KeystoneIdentity {
    pub token: Zeroizing<String>,
    pub user_id: String,
    pub name: String,
    pub expires_at: DateTime<Utc>,
}

pub(super) struct ProjectAuthority {
    pub targets: Vec<CatalogTarget>,
    pub keys: Vec<Ec2Credential>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub(super) struct Ec2Credential {
    pub access: String,
    pub secret: String,
    user_id: String,
    tenant_id: Option<String>,
    project_id: Option<String>,
}

#[derive(Deserialize)]
struct TokenDocument {
    token: Token,
}

#[derive(Deserialize)]
struct Token {
    user: User,
    expires_at: DateTime<Utc>,
    methods: Vec<String>,
    project: Option<ProjectScope>,
    #[serde(default)]
    catalog: Vec<CatalogService>,
    #[serde(rename = "OS-TRUST:trust")]
    trust: Option<Value>,
    application_credential: Option<Value>,
    #[serde(rename = "OS-OAUTH1")]
    oauth: Option<Value>,
}

#[derive(Deserialize)]
struct User {
    id: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct ProjectScope {
    id: String,
}

#[derive(Deserialize)]
struct CatalogService {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    endpoints: Vec<CatalogEndpoint>,
}

#[derive(Deserialize)]
struct CatalogEndpoint {
    id: String,
    interface: String,
    region_id: Option<String>,
    region: Option<String>,
    url: String,
}

#[derive(Deserialize)]
struct ProjectPage {
    projects: Vec<Project>,
    links: Option<ProjectLinks>,
}

#[derive(Deserialize)]
struct ProjectLinks {
    next: Option<String>,
}

#[derive(Deserialize)]
struct CredentialDocument {
    credentials: Vec<Ec2Credential>,
}

impl KeystoneRepository {
    pub fn new(auth_url: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| unavailable())?;
        Ok(Self {
            client,
            base: keystone_url(auth_url)?,
        })
    }

    pub async fn authenticate(
        &self,
        authentication: Authentication,
        default_domain: &str,
    ) -> Result<KeystoneIdentity> {
        let (request_identity, primary) = match authentication {
            Authentication::Password {
                username,
                domain,
                password,
            } => {
                let domain = if domain.trim().is_empty() {
                    default_domain
                } else {
                    domain.trim()
                };
                if username.trim().is_empty() || domain.is_empty() || password.is_empty() {
                    return Err(AppError::Invalid(
                        "Enter a Keystone username, user domain and password.".into(),
                    ));
                }
                (
                    json!({"methods": ["password"], "password": {"user": {
                        "name": username.trim(), "domain": {"name": domain}, "password": password
                    }}}),
                    None,
                )
            }
            Authentication::Token { token } => {
                let token = token.trim();
                if token.is_empty() {
                    return Err(AppError::Invalid(
                        "Enter a primary Keystone user token.".into(),
                    ));
                }
                let request = self
                    .client
                    .get(self.url(&["auth", "tokens"])?)
                    .header("X-Auth-Token", token)
                    .header("X-Subject-Token", token);
                let source: TokenDocument = read_response(send(request).await?).await?;
                source.token.check_primary()?;
                (token_method(token), Some(source.token))
            }
        };
        let (identity, payload) = self.issue(request_identity, json!("unscoped")).await?;
        if payload.project.is_some()
            || primary
                .as_ref()
                .is_some_and(|source| source.user.id != identity.user_id)
        {
            return Err(identity_mismatch());
        }
        Ok(KeystoneIdentity {
            expires_at: primary.map_or(identity.expires_at, |source| {
                source.expires_at.min(identity.expires_at)
            }),
            ..identity
        })
    }

    pub async fn projects(&self, identity: &KeystoneIdentity) -> Result<Vec<Project>> {
        let endpoint = self.url(&["auth", "projects"])?;
        let mut next = endpoint.clone();
        let mut seen = std::collections::HashSet::new();
        let mut projects = Vec::new();
        for _ in 0..20 {
            if !seen.insert(next.clone()) {
                return Err(invalid_response());
            }
            let page: ProjectPage = self.get(next, &identity.token).await?;
            if projects.len() + page.projects.len() > 10_000 {
                return Err(invalid_response());
            }
            for project in page.projects.into_iter().filter(|project| project.enabled) {
                validate_identifier(&project.id)?;
                projects.push(project);
            }
            let Some(link) = page
                .links
                .and_then(|links| links.next)
                .filter(|link| !link.is_empty())
            else {
                projects.sort_by(|first, second| {
                    (&first.name, &first.id).cmp(&(&second.name, &second.id))
                });
                return Ok(projects);
            };
            next = endpoint.join(&link).map_err(|_| invalid_response())?;
            if next.origin() != endpoint.origin()
                || next.path() != endpoint.path()
                || !next.username().is_empty()
                || next.password().is_some()
                || next.fragment().is_some()
            {
                return Err(invalid_response());
            }
        }
        Err(invalid_response())
    }

    pub async fn project_authority(
        &self,
        identity: &KeystoneIdentity,
        project: &str,
    ) -> Result<ProjectAuthority> {
        let (scoped, payload) = self
            .issue(
                token_method(&identity.token),
                json!({"project": {"id": project}}),
            )
            .await?;
        if scoped.user_id != identity.user_id
            || payload.project.as_ref().map(|scope| scope.id.as_str()) != Some(project)
        {
            return Err(identity_mismatch());
        }
        let targets = catalog_targets(payload.catalog)?;
        let url = self.url(&["users", &identity.user_id, "credentials", "OS-EC2"])?;
        let credentials: CredentialDocument = self.get(url, &scoped.token).await?;
        let keys = project_keys(credentials.credentials, &identity.user_id, project)?;
        Ok(ProjectAuthority {
            targets,
            keys,
            expires_at: scoped.expires_at.min(identity.expires_at),
        })
    }

    async fn issue(&self, identity: Value, scope: Value) -> Result<(KeystoneIdentity, Token)> {
        let request = self
            .client
            .request(Method::POST, self.url(&["auth", "tokens"])?)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(json!({"auth": {"identity": identity, "scope": scope}}).to_string());
        let response = send(request).await?;
        let token = response
            .headers()
            .get("X-Subject-Token")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(invalid_response)?;
        validate_identifier(token)?;
        let token = Zeroizing::new(token.to_owned());
        let document: TokenDocument = read_response(response).await?;
        document.token.check_primary()?;
        let user = &document.token.user;
        Ok((
            KeystoneIdentity {
                token,
                user_id: user.id.clone(),
                name: user.name.clone().unwrap_or_else(|| user.id.clone()),
                expires_at: document.token.expires_at,
            },
            document.token,
        ))
    }

    async fn get<T: DeserializeOwned>(&self, url: Url, token: &str) -> Result<T> {
        read_response(send(self.client.get(url).header("X-Auth-Token", token)).await?).await
    }

    fn url(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| invalid_response())?
            .pop_if_empty()
            .extend(segments);
        Ok(url)
    }
}

impl Token {
    fn check_primary(&self) -> Result<()> {
        const PRIMARY: &[&str] = &[
            "password", "token", "totp", "mapped", "saml2", "openid", "external", "kerberos",
            "x509",
        ];
        if self.trust.is_some()
            || self.application_credential.is_some()
            || self.oauth.is_some()
            || self.methods.is_empty()
            || self
                .methods
                .iter()
                .any(|method| !PRIMARY.contains(&method.as_str()))
        {
            return Err(AppError::Invalid(
                "A primary Keystone user token is required.".into(),
            ));
        }
        if self.expires_at <= Utc::now() {
            return Err(reconnect());
        }
        validate_identifier(&self.user.id)
    }
}

pub(super) fn keystone_url(value: &str) -> Result<Url> {
    let url = https_url(value);
    #[cfg(test)]
    let url = url.or_else(|| {
        let local = Url::parse(value).ok()?;
        (local.scheme() == "http" && local.host_str() == Some("127.0.0.1")).then_some(local)
    });
    url.filter(|url| url.path().trim_end_matches('/').ends_with("/v3"))
        .ok_or_else(|| AppError::Invalid("Enter an HTTPS Keystone URL ending in /v3.".into()))
}

pub(super) fn normalized_endpoint(value: &str) -> Option<String> {
    let url = https_url(value)?;
    let path = percent_decode_str(url.path())
        .decode_utf8()
        .ok()?
        .to_ascii_lowercase();
    if path.contains(['{', '}', '%', '\\'])
        || path
            .split('/')
            .any(|segment| matches!(segment, "v1" | "swift") || segment.starts_with("auth_"))
    {
        return None;
    }
    Some(url.as_str().trim_end_matches('/').to_owned())
}

fn https_url(value: &str) -> Option<Url> {
    let url = Url::parse(value.trim()).ok()?;
    (url.scheme() == "https"
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

fn catalog_targets(catalog: Vec<CatalogService>) -> Result<Vec<CatalogTarget>> {
    if catalog.len() > 512
        || catalog
            .iter()
            .map(|service| service.endpoints.len())
            .sum::<usize>()
            > 4096
    {
        return Err(invalid_response());
    }
    let mut explicit = Vec::new();
    let mut fallback = Vec::new();
    for service in catalog {
        let targets = match service.kind.to_ascii_lowercase().as_str() {
            "s3" => &mut explicit,
            "object-store" => &mut fallback,
            _ => continue,
        };
        for endpoint in service.endpoints {
            if matches!(endpoint.interface.as_str(), "public" | "internal")
                && let Some(url) = normalized_endpoint(&endpoint.url)
            {
                validate_identifier(&endpoint.id)?;
                let region = endpoint
                    .region_id
                    .or(endpoint.region)
                    .unwrap_or_else(|| "default".into());
                validate_identifier(&region)?;
                targets.push(CatalogTarget {
                    id: endpoint.id,
                    region,
                    endpoint: url,
                    interface: endpoint.interface,
                });
            }
        }
    }
    let mut targets = if explicit.is_empty() {
        fallback
    } else {
        explicit
    };
    let mut ids = std::collections::HashSet::new();
    if targets.iter().any(|target| !ids.insert(target.id.clone())) {
        return Err(invalid_response());
    }
    targets.sort_by(|first, second| {
        (&first.region, &first.endpoint, &first.id).cmp(&(
            &second.region,
            &second.endpoint,
            &second.id,
        ))
    });
    Ok(targets)
}

fn project_keys(rows: Vec<Ec2Credential>, user: &str, project: &str) -> Result<Vec<Ec2Credential>> {
    let mut keys = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        if row.user_id != user
            || matches!((&row.tenant_id, &row.project_id), (Some(tenant), Some(project)) if tenant != project)
        {
            return Err(identity_mismatch());
        }
        if row.tenant_id.as_deref().or(row.project_id.as_deref()) == Some(project) {
            validate_identifier(&row.access)?;
            validate_identifier(&row.secret)?;
            if !seen.insert(row.access.clone()) {
                return Err(invalid_response());
            }
            keys.push(row);
        }
    }
    keys.sort_by(|first, second| first.access.cmp(&second.access));
    Ok(keys)
}

fn token_method(token: &str) -> Value {
    json!({"methods": ["token"], "token": {"id": token}})
}

async fn send(request: reqwest::RequestBuilder) -> Result<Response> {
    let response = request
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| unavailable())?;
    match response.status() {
        StatusCode::UNAUTHORIZED => Err(reconnect()),
        StatusCode::FORBIDDEN => Err(AppError::Invalid("Keystone denied this operation.".into())),
        status if status.is_success() => Ok(response),
        _ => Err(unavailable()),
    }
}

async fn read_response<T: DeserializeOwned>(mut response: Response) -> Result<T> {
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(invalid_response());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| invalid_response())
}

fn validate_identifier(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 8192 || value.chars().any(char::is_control) {
        return Err(invalid_response());
    }
    Ok(())
}

fn identity_mismatch() -> AppError {
    AppError::Invalid("Keystone returned an identity outside the selected user or project.".into())
}

fn invalid_response() -> AppError {
    AppError::Storage("Keystone returned an invalid response.".into())
}

fn unavailable() -> AppError {
    AppError::Storage("Keystone could not complete the request.".into())
}

pub(super) fn reconnect() -> AppError {
    AppError::Reauthenticate("Reconnect to OpenStack to continue.".into())
}
