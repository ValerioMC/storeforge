use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::{
    audit::service::AuditService, common::error::Result, connection::service::ConnectionService,
};

use super::{
    model::AdminRequest,
    private_access::{PrivateAccess, PrivateAccessChange, PrivateAccessService},
    service::AdministrationService,
};

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

#[tauri::command]
pub async fn list_private_iam(
    service: State<'_, PrivateAccessService>,
    connection: Uuid,
) -> Result<Vec<PrivateAccess>> {
    service.list(connection).await
}

#[tauri::command]
pub async fn change_private_iam(
    service: State<'_, PrivateAccessService>,
    iam: State<'_, AdministrationService>,
    connections: State<'_, Arc<ConnectionService>>,
    audit: State<'_, AuditService>,
    connection: Uuid,
    change: PrivateAccessChange,
) -> Result<Option<PrivateAccess>> {
    let action = match &change {
        PrivateAccessChange::Create { .. } => "Provision private IAM connection",
        PrivateAccessChange::Retry { .. } => "Retry private IAM connection",
        PrivateAccessChange::Retire { .. } => "Retire private IAM connection",
    };
    let result = service.change(&iam, &connections, connection, change).await;
    audit.record(connection, action, None, result)
}
