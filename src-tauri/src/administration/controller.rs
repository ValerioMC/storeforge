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
    let (workspace, identity) = service.execution_identity(&request.operation)?;
    let session = connections.session_for(connection, workspace, identity)?;
    service.execute(&session, request, &audit).await
}
