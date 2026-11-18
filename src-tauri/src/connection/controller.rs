use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::{bucket::service::BucketService, common::error::Result};

use super::{
    model::{Connection, ConnectionDraft},
    service::ConnectionService,
};

#[tauri::command]
pub fn list_connections(service: State<'_, Arc<ConnectionService>>) -> Result<Vec<Connection>> {
    service.list()
}

#[tauri::command]
pub fn save_connection(
    service: State<'_, Arc<ConnectionService>>,
    draft: ConnectionDraft,
) -> Result<Connection> {
    service.save(draft)
}

#[tauri::command]
pub fn remove_connection(service: State<'_, Arc<ConnectionService>>, id: Uuid) -> Result<()> {
    service.remove(id)
}

#[tauri::command]
pub async fn test_connection(
    service: State<'_, Arc<ConnectionService>>,
    draft: ConnectionDraft,
) -> Result<usize> {
    let session = service.draft_session(draft)?;
    Ok(BucketService::list(&session).await?.len())
}
