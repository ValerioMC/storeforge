use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::{
    common::error::Result, connection::service::ConnectionService, object::model::Location,
};

use super::{
    inventory,
    model::{Comparison, Integrity, Job, Transfer, Usage},
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
