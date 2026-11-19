#[path = "governance.rs"]
pub(super) mod governance;
#[path = "sharing.rs"]
mod sharing;

use std::path::{Path, PathBuf};

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::{Method, RequestBuilder, header, multipart};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;
use url::Url;
use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::shared_service::{
        repository::{SharedServiceRepository, read_body},
        service::SharedService,
    },
};

use super::model::{PortalChange, PortalRead, SpaceAccess, SpaceRole, SpaceStatus};

pub(crate) async fn read(
    service: &SharedService,
    id: Uuid,
    project: Option<u64>,
    space: Option<String>,
    operation: PortalRead,
) -> Result<Value> {
    service
        .scoped(
            id,
            |session| session.access.portal.available,
            async |remote, origin, token| {
                let portal = PortalSession {
                    remote,
                    origin,
                    token,
                };
                if matches!(operation, PortalRead::Projects) {
                    return portal
                        .get("/api/portal/accounts", &[] as &[(&str, &str)])
                        .await;
                }
                ProjectSession::new(portal, project.unwrap_or_default(), space)?
                    .read(operation)
                    .await
            },
        )
        .await
}

pub(crate) async fn change(
    service: &SharedService,
    id: Uuid,
    project: u64,
    space: Option<String>,
    operation: PortalChange,
) -> Result<Value> {
    service
        .scoped(
            id,
            |session| session.access.portal.available,
            async |remote, origin, token| {
                ProjectSession::new(
                    PortalSession {
                        remote,
                        origin,
                        token,
                    },
                    project,
                    space,
                )?
                .change(operation)
                .await
            },
        )
        .await
}

pub(crate) async fn file(
    service: &SharedService,
    id: Uuid,
    project: u64,
    space: String,
    key: String,
    version: Option<String>,
    destination: Option<PathBuf>,
) -> Result<Vec<u8>> {
    let payload = service
        .scoped(
            id,
            |session| session.access.portal.available,
            async |remote, origin, token| {
                let session = ProjectSession::new(
                    PortalSession {
                        remote,
                        origin,
                        token,
                    },
                    project,
                    Some(space),
                )?;
                session
                    .download(&key, version.as_deref(), destination.as_deref())
                    .await
            },
        )
        .await?;
    match payload {
        Download::Preview(bytes) => Ok(bytes),
        Download::File(temporary) => {
            temporary
                .persist(
                    destination
                        .ok_or_else(|| AppError::Invalid("Choose a file destination.".into()))?,
                )
                .map_err(|failure| AppError::from(failure.error))?;
            Ok(Vec::new())
        }
    }
}

pub(crate) async fn upload(
    service: &SharedService,
    id: Uuid,
    project: u64,
    space: String,
    key: String,
    source: PathBuf,
) -> Result<()> {
    service
        .scoped(
            id,
            |session| session.access.portal.available,
            async |remote, origin, token| {
                let session = ProjectSession::new(
                    PortalSession {
                        remote,
                        origin,
                        token,
                    },
                    project,
                    Some(space),
                )?;
                session.upload(key, &source).await
            },
        )
        .await
}

enum Download {
    Preview(Vec<u8>),
    File(NamedTempFile),
}

struct PortalSession<'a> {
    remote: &'a SharedServiceRepository,
    origin: &'a Url,
    token: &'a str,
}

impl PortalSession<'_> {
    async fn get<T: DeserializeOwned, Q: Serialize + ?Sized>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T> {
        self.remote
            .get(self.origin, path, query, Some(self.token))
            .await
    }

    fn request(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        self.remote
            .request(method, self.origin, path, Some(self.token))
    }
}

struct ProjectSession<'a> {
    portal: PortalSession<'a>,
    account: String,
    space: Option<String>,
}

impl<'a> ProjectSession<'a> {
    fn new(portal: PortalSession<'a>, project: u64, space: Option<String>) -> Result<Self> {
        if project == 0 {
            return Err(AppError::Invalid("Select a Portal project.".into()));
        }
        let space = space.as_deref().map(route_segment).transpose()?;
        Ok(Self {
            portal,
            account: project.to_string(),
            space,
        })
    }

    fn space_path(&self, suffix: &str) -> Result<String> {
        let space = self
            .space
            .as_deref()
            .ok_or_else(|| AppError::Invalid("Select a storage space.".into()))?;
        Ok(format!("/api/portal/storage-spaces/{space}{suffix}"))
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, parameters: &[(&str, &str)]) -> Result<T> {
        let mut query = vec![("account_id", self.account.as_str())];
        query.extend_from_slice(parameters);
        self.portal.get(path, &query).await
    }

    async fn read(&self, operation: PortalRead) -> Result<Value> {
        match operation {
            PortalRead::Governance { view } => self.read_governance(view).await,
            PortalRead::Sharing { view } => self.read_sharing(view).await,
            PortalRead::Projects => Err(AppError::Invalid(
                "Project catalog requests are unscoped.".into(),
            )),
            PortalRead::State => self.get("/api/portal/state", &[]).await,
            PortalRead::Usage => self.get("/api/portal/usage", &[]).await,
            PortalRead::Trends => {
                self.get("/api/portal/usage-history-trends", &[("window", "month")])
                    .await
            }
            PortalRead::Health => self.get("/api/portal/endpoint-health", &[]).await,
            PortalRead::Alerts => self.get("/api/portal/alerts", &[]).await,
            PortalRead::Activity => self.get("/api/portal/activity", &[("limit", "12")]).await,
            PortalRead::Spaces => {
                self.get(
                    "/api/portal/storage-spaces",
                    &[("include_archived", "true")],
                )
                .await
            }
            PortalRead::Space => self.get(&self.space_path("")?, &[]).await,
            PortalRead::Statistics => self.get(&self.space_path("/usage-stats")?, &[]).await,
            PortalRead::SpaceSettings => self.get(&self.space_path("/settings")?, &[]).await,
            PortalRead::Detail { key } => {
                self.get(&self.space_path("/objects/detail")?, &[("key", &key)])
                    .await
            }
            PortalRead::Files { prefix, cursor } => self.files(&prefix, cursor.as_deref()).await,
            PortalRead::Trash {
                key_marker,
                version_id_marker,
            } => {
                self.versions(
                    "/trash",
                    None,
                    key_marker.as_deref(),
                    version_id_marker.as_deref(),
                )
                .await
            }
            PortalRead::Versions {
                key,
                key_marker,
                version_id_marker,
            } => {
                self.versions(
                    "/objects/versions",
                    Some(&key),
                    key_marker.as_deref(),
                    version_id_marker.as_deref(),
                )
                .await
            }
            PortalRead::Candidates { search, page } => {
                let path = match self.space {
                    Some(_) => self.space_path("/share-candidates")?,
                    None => "/api/portal/share-candidates".into(),
                };
                self.get(
                    &path,
                    &[
                        ("search", &search),
                        ("page", &page.to_string()),
                        ("page_size", "30"),
                    ],
                )
                .await
            }
        }
    }

    async fn files(&self, prefix: &str, cursor: Option<&str>) -> Result<Value> {
        let access = self.space_access().await?;
        let request = self
            .browser_request(Method::GET, &access, "objects")?
            .query(&[("prefix", prefix), ("max_keys", "100")]);
        let request = match cursor {
            Some(cursor) => request.query(&[("continuation_token", cursor)]),
            None => request,
        };
        SharedServiceRepository::json(request).await
    }

    async fn versions(
        &self,
        suffix: &str,
        key: Option<&str>,
        marker: Option<&str>,
        version: Option<&str>,
    ) -> Result<Value> {
        let mut query = vec![("max_keys", "100")];
        query.extend(
            [
                ("key", key),
                ("key_marker", marker),
                ("version_id_marker", version),
            ]
            .into_iter()
            .filter_map(|(name, value)| value.map(|value| (name, value))),
        );
        self.get(&self.space_path(suffix)?, &query).await
    }

    async fn change(&self, operation: PortalChange) -> Result<Value> {
        match operation {
            PortalChange::Governance { action } => self.change_governance(action).await,
            PortalChange::Sharing { action } => self.change_sharing(action).await,
            PortalChange::Create { draft } => {
                self.submit(Method::POST, "/api/portal/storage-spaces", &draft)
                    .await
            }
            PortalChange::Import { draft } => {
                self.submit(Method::POST, "/api/portal/storage-spaces/import", &draft)
                    .await
            }
            PortalChange::Edit { draft } => self.update_space(Method::PATCH, "", &draft).await,
            PortalChange::Archive { archived } => {
                self.update_space(Method::PATCH, "", &json!({ "archived": archived }))
                    .await
            }
            PortalChange::TakeOwnership => {
                self.update_space(Method::POST, "/take-ownership", &())
                    .await
            }
            PortalChange::Delete { confirmation } => self.delete_space(&confirmation).await,
            PortalChange::Icon { draft } => self.update_space(Method::PUT, "/icon", &draft).await,
            PortalChange::SpaceSettings { draft } => {
                self.update_space(Method::PUT, "/settings", &draft).await
            }
            PortalChange::Folder { prefix } => self.create_folder(&prefix).await,
            PortalChange::DeleteFile { key } => self.delete_file(&key).await,
            PortalChange::RestoreVersion { key, version_id } => {
                self.writable_space().await?;
                self.update_space(
                    Method::POST,
                    "/objects/restore",
                    &json!({ "key": key, "version_id": version_id }),
                )
                .await
            }
            PortalChange::RestorePrefix { prefix } => {
                self.writable_space().await?;
                self.stream("/trash/restore-prefix/stream", &json!({ "prefix": prefix }))
                    .await
            }
            PortalChange::Cleanup { confirmation } => {
                self.writable_space().await?;
                self.stream(
                    "/versions/cleanup/stream",
                    &json!({ "confirmation": confirmation }),
                )
                .await
            }
        }
    }

    async fn submit<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        draft: &impl Serialize,
    ) -> Result<T> {
        let request = self
            .portal
            .remote
            .change_request(method, self.portal.origin, path, self.portal.token, draft)?
            .query(&[("account_id", &self.account)]);
        SharedServiceRepository::json(request).await
    }

    async fn update_space(
        &self,
        method: Method,
        suffix: &str,
        draft: &impl Serialize,
    ) -> Result<Value> {
        self.submit(method, &self.space_path(suffix)?, draft).await
    }

    async fn delete_space(&self, confirmation: &str) -> Result<Value> {
        let access = self.space_access().await?;
        if !access.can_delete || confirmation != access.name {
            return Err(AppError::Invalid(
                "Type the space name to confirm empty-only deletion.".into(),
            ));
        }
        let request = self
            .portal
            .request(Method::DELETE, &self.space_path("")?)?
            .query(&[("account_id", &self.account)]);
        SharedServiceRepository::send(request).await?;
        Ok(Value::Null)
    }

    async fn delete_file(&self, key: &str) -> Result<Value> {
        self.writable_space().await?;
        let request = self
            .portal
            .request(Method::DELETE, &self.space_path("/objects")?)?
            .query(&[("account_id", self.account.as_str()), ("key", key)]);
        SharedServiceRepository::json(request).await
    }

    async fn create_folder(&self, prefix: &str) -> Result<Value> {
        let access = self.writable_space().await?;
        let request = self
            .browser_request(Method::POST, &access, "folders")?
            .header(header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&json!({ "prefix": prefix }))?);
        SharedServiceRepository::send(request).await?;
        Ok(Value::Null)
    }

    async fn space_access(&self) -> Result<SpaceAccess> {
        self.get(&self.space_path("")?, &[]).await
    }

    async fn writable_space(&self) -> Result<SpaceAccess> {
        let space = self.space_access().await?;
        if space.status == SpaceStatus::Archived
            || !space.can_browse
            || matches!(space.role, SpaceRole::Viewer)
        {
            return Err(AppError::AuthorizationDenied(
                "This storage space is read-only.".into(),
            ));
        }
        Ok(space)
    }

    fn browser_request(
        &self,
        method: Method,
        space: &SpaceAccess,
        operation: &str,
    ) -> Result<RequestBuilder> {
        if !space.can_browse {
            return Err(AppError::AuthorizationDenied(
                "File access is unavailable for this storage space.".into(),
            ));
        }
        let bucket = space.internal_bucket_name.as_deref().ok_or_else(|| {
            AppError::Missing("The space has no published storage bucket.".into())
        })?;
        let path = format!(
            "/api/browser/buckets/{}/{}",
            route_segment(bucket)?,
            operation
        );
        Ok(self
            .portal
            .request(method, &path)?
            .header("X-S3-Workspace", "portal")
            .query(&[("context_id", &self.account)]))
    }

    async fn stream(&self, suffix: &str, draft: &impl Serialize) -> Result<Value> {
        let request = self
            .portal
            .remote
            .change_request(
                Method::POST,
                self.portal.origin,
                &self.space_path(suffix)?,
                self.portal.token,
                draft,
            )?
            .header(header::ACCEPT, "text/event-stream")
            .query(&[("account_id", &self.account)])
            .timeout(std::time::Duration::from_secs(1800));
        read_operation_result(SharedServiceRepository::send(request).await?).await
    }

    async fn upload(&self, key: String, source: &Path) -> Result<()> {
        let access = self.writable_space().await?;
        if !source.is_absolute() || !tokio::fs::metadata(source).await?.is_file() {
            return Err(AppError::Invalid("Choose a local file to upload.".into()));
        }
        let body = multipart::Form::new()
            .text("key", key)
            .file("file", source)
            .await?;
        let request = self
            .browser_request(Method::POST, &access, "proxy-upload")?
            .multipart(body)
            .timeout(std::time::Duration::from_secs(3600));
        SharedServiceRepository::send(request).await?;
        Ok(())
    }

    async fn download(
        &self,
        key: &str,
        version: Option<&str>,
        destination: Option<&Path>,
    ) -> Result<Download> {
        let mut request = self
            .portal
            .request(Method::GET, &self.space_path("/objects/download")?)?
            .query(&[("account_id", self.account.as_str()), ("key", key)]);
        if let Some(version) = version {
            request = request.query(&[("version_id", version)]);
        }
        if destination.is_some() {
            request = request.timeout(std::time::Duration::from_secs(3600));
        }
        let response = SharedServiceRepository::send(request).await?;
        match destination {
            Some(path) => save_download(response, path).await.map(Download::File),
            None => read_body(response).await.map(Download::Preview),
        }
    }
}

async fn save_download(
    mut response: reqwest::Response,
    destination: &Path,
) -> Result<NamedTempFile> {
    let directory = destination
        .parent()
        .filter(|_| destination.is_absolute())
        .ok_or_else(|| AppError::Invalid("Choose an absolute download destination.".into()))?;
    let temporary = NamedTempFile::new_in(directory)?;
    let mut target = tokio::fs::File::from_std(temporary.reopen()?);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppError::Storage("The Portal download was interrupted.".into()))?
    {
        target.write_all(&chunk).await?;
    }
    target.flush().await?;
    Ok(temporary)
}

async fn read_operation_result(mut response: reqwest::Response) -> Result<Value> {
    let mut pending = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| unknown_operation_result())?
    {
        pending.extend_from_slice(&chunk);
        while let Some(end) = pending.windows(2).position(|pair| pair == b"\n\n") {
            let event: Vec<_> = pending.drain(..end + 2).collect();
            if let Some(result) = operation_event(&event)? {
                return Ok(result);
            }
        }
        if pending.len() > 1024 * 1024 {
            return Err(unknown_operation_result());
        }
    }
    Err(unknown_operation_result())
}

fn operation_event(event: &[u8]) -> Result<Option<Value>> {
    let frame = std::str::from_utf8(event).map_err(|_| unknown_operation_result())?;
    let name = frame.lines().find_map(|line| line.strip_prefix("event: "));
    let payload = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .collect::<Vec<_>>()
        .join("\n");
    match name {
        Some("result") => serde_json::from_str(&payload)
            .map(Some)
            .map_err(|_| unknown_operation_result()),
        Some("error") => {
            #[derive(serde::Deserialize)]
            struct Failure {
                detail: String,
            }
            let message = serde_json::from_str::<Failure>(&payload)
                .map_err(|_| unknown_operation_result())?
                .detail;
            Err(AppError::Storage(message))
        }
        Some("done") => Err(unknown_operation_result()),
        _ => Ok(None),
    }
}

fn unknown_operation_result() -> AppError {
    AppError::AmbiguousWrite("The service did not confirm the operation result. Refresh the space and activity before retrying.".into())
}

fn route_segment(value: &str) -> Result<String> {
    if value.is_empty() || matches!(value, "." | "..") || value.contains(['/', '\\', '%']) {
        return Err(AppError::Invalid(
            "Invalid Portal resource identifier.".into(),
        ));
    }
    const PATH_SEGMENT: &percent_encoding::AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    Ok(utf8_percent_encode(value, PATH_SEGMENT).to_string())
}
