use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::{
    common::error::Result,
    connection::service::ConnectionService,
    object::model::{Location, ObjectVersion},
};

use super::{
    batch, inventory,
    model::{Comparison, Integrity, Job, Transfer, Usage, VersionPlanItem, VersionRule},
    service::TransferService,
};

#[tauri::command]
pub fn list_jobs(transfers: State<'_, TransferService>) -> Result<Vec<Job>> {
    transfers.list()
}

#[tauri::command]
pub fn start_transfer(transfers: State<'_, TransferService>, transfer: Transfer) -> Result<Uuid> {
    transfers.start(transfer)
}

#[tauri::command]
pub fn cancel_job(transfers: State<'_, TransferService>, id: Uuid) -> Result<()> {
    transfers.cancel(id)
}

#[tauri::command]
pub async fn compare_locations(
    connections: State<'_, Arc<ConnectionService>>,
    source: Location,
    target: Location,
) -> Result<Comparison> {
    let source_session = connections.session(source.connection)?;
    let target_session = connections.session(target.connection)?;
    inventory::compare(&source_session.s3, &source, &target_session.s3, &target).await
}

#[tauri::command]
pub async fn location_usage(
    connections: State<'_, Arc<ConnectionService>>,
    location: Location,
    include_versions: bool,
) -> Result<Usage> {
    inventory::usage(
        &connections.session(location.connection)?.s3,
        &location,
        include_versions,
    )
    .await
}

#[tauri::command]
pub async fn check_integrity(
    connections: State<'_, Arc<ConnectionService>>,
    location: Location,
) -> Result<Integrity> {
    inventory::integrity(&connections.session(location.connection)?.s3, &location).await
}

#[tauri::command]
pub async fn prefix_history(
    connections: State<'_, Arc<ConnectionService>>,
    location: Location,
) -> Result<Vec<ObjectVersion>> {
    batch::prefix_history(&connections.session(location.connection)?.s3, &location).await
}

#[tauri::command]
pub async fn preview_version_plan(
    connections: State<'_, Arc<ConnectionService>>,
    location: Location,
    rule: VersionRule,
) -> Result<Vec<VersionPlanItem>> {
    let history =
        batch::prefix_history(&connections.session(location.connection)?.s3, &location).await?;
    batch::version_plan(&history, &rule)
}

#[tauri::command]
pub async fn export_transfer_report(path: std::path::PathBuf, report: String) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        crate::common::error::AppError::Invalid("Choose a report destination.".into())
    })?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    tokio::fs::write(temporary.path(), report).await?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}
