use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::{
    audit::service::AuditService, common::error::Result, connection::service::ConnectionService,
};

use super::{
    model::AdminRequest,
    private_access::{PrivateAccess, PrivateAccessChange, PrivateAccessService},
    rgw_identities::{
        RgwIdentityService,
        model::{ChangeRequest, ChangeResponse, KeyChange, ReadRequest, ReadResponse},
    },
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

#[tauri::command]
pub async fn read_rgw_identity(
    service: State<'_, RgwIdentityService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    request: ReadRequest,
) -> Result<ReadResponse> {
    service.read(&connections, connection, request).await
}

#[tauri::command]
pub async fn change_rgw_identity(
    service: State<'_, RgwIdentityService>,
    connections: State<'_, Arc<ConnectionService>>,
    audit: State<'_, AuditService>,
    connection: Uuid,
    request: ChangeRequest,
) -> Result<ChangeResponse> {
    let action = match &request {
        ChangeRequest::ImportAccounts { .. } => "Register RGW accounts",
        ChangeRequest::ImportUsers { .. } => "Register RGW users",
        ChangeRequest::CreateAccount(_) => "Create RGW account",
        ChangeRequest::UpdateAccount { .. } => "Update RGW account",
        ChangeRequest::CreateUser { .. } => "Create RGW user",
        ChangeRequest::UpdateUser { .. } => "Update RGW user",
        ChangeRequest::SetQuota { .. } => "Set RGW quota",
        ChangeRequest::Key { change, .. } => match change {
            KeyChange::Create { .. } => "Create RGW key",
            KeyChange::Metadata { .. } => "Update RGW key metadata",
            KeyChange::Status { .. } => "Change RGW key status",
            KeyChange::Delete { .. } => "Delete RGW key",
        },
    };
    audit.record(
        connection,
        action,
        None,
        service.change(&connections, connection, request).await,
    )
}
