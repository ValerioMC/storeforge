use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::{
    audit::service::AuditService, common::error::Result, connection::service::ConnectionService,
};

use super::{model::AdminRequest, service::AdministrationService};

#[tauri::command]
pub async fn administer(
    service: State<'_, AdministrationService>,
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    request: AdminRequest,
) -> Result<String> {
    service
        .execute(&connections.session(connection)?, request, &audit)
        .await
}
