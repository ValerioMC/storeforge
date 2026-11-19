use serde_json::Value;
use tauri::{State, ipc::Response};
use uuid::Uuid;

use crate::{common::error::Result, connection::shared_service::service::SharedService};

use super::{
    model::{PortalChange, PortalRead},
    service,
};

#[tauri::command]
pub async fn read_portal(
    service: State<'_, SharedService>,
    id: Uuid,
    project: Option<u64>,
    space: Option<String>,
    operation: PortalRead,
) -> Result<Value> {
    service::read(&service, id, project, space, operation).await
}

#[tauri::command]
pub async fn change_portal(
    service: State<'_, SharedService>,
    id: Uuid,
    project: u64,
    space: Option<String>,
    operation: PortalChange,
) -> Result<Value> {
    service::change(&service, id, project, space, operation).await
}

#[tauri::command]
pub async fn portal_preview(
    service: State<'_, SharedService>,
    id: Uuid,
    project: u64,
    space: String,
    key: String,
) -> Result<Response> {
    service::file(&service, id, project, space, key, None, None)
        .await
        .map(Response::new)
}

#[tauri::command]
pub async fn portal_download(
    service: State<'_, SharedService>,
    id: Uuid,
    project: u64,
    space: String,
    key: String,
    version: Option<String>,
    destination: std::path::PathBuf,
) -> Result<()> {
    service::file(
        &service,
        id,
        project,
        space,
        key,
        version,
        Some(destination),
    )
    .await
    .map(drop)
}

#[tauri::command]
pub async fn portal_upload(
    service: State<'_, SharedService>,
    id: Uuid,
    project: u64,
    space: String,
    key: String,
    source: std::path::PathBuf,
) -> Result<()> {
    service::upload(&service, id, project, space, key, source).await
}
