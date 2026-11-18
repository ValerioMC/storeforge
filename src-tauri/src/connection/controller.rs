use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::common::error::Result;

use super::{
    model::{Connection, ConnectionCheck, ConnectionDraft},
    service::ConnectionService,
};

#[tauri::command]
pub fn list_connections(service: State<'_, Arc<ConnectionService>>) -> Result<Vec<Connection>> {
    service.list()
}

#[tauri::command]
pub async fn save_connection(
    service: State<'_, Arc<ConnectionService>>,
    draft: ConnectionDraft,
) -> Result<Connection> {
    service.save(draft).await
}

#[tauri::command]
pub fn remove_connection(service: State<'_, Arc<ConnectionService>>, id: Uuid) -> Result<()> {
    service.remove(id)
}

#[tauri::command]
pub async fn test_connection(
    service: State<'_, Arc<ConnectionService>>,
    draft: ConnectionDraft,
) -> Result<ConnectionCheck> {
    service.check(draft).await
}
