use std::sync::Arc;

use tauri::{State, ipc::Channel};
use uuid::Uuid;

use crate::{
    audit::service::AuditService,
    bucket::workbench::{BucketTarget, validate_revision},
    common::error::{AppError, Result},
    connection::{
        model::{IdentityKind, Workspace},
        service::ConnectionService,
    },
};

use super::{
    ceph_operations::{self, Action, BucketOwner, IndexOutcome, Outcome},
    model::AdminRequest,
    private_access::{PrivateAccess, PrivateAccessChange, PrivateAccessService},
    rgw_identities::{
        RgwIdentityService,
        model::{
            ChangeRequest, ChangeResponse, IdentityTarget, KeyChange, ReadRequest, ReadResponse,
        },
    },
    service::AdministrationService,
};

#[tauri::command]
pub async fn ceph_link_owner(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    revision: Option<Uuid>,
    target: IdentityTarget,
) -> Result<BucketOwner> {
    let session =
        connections.session_for(connection, Workspace::CephAdmin, IdentityKind::CephAdmin)?;
    validate_revision(&session, revision)?;
    ceph_operations::resolve_owner(&session, target).await
}

#[tauri::command]
pub async fn ceph_action(
    connections: State<'_, Arc<ConnectionService>>,
    audit: State<'_, AuditService>,
    connection: Uuid,
    revision: Option<Uuid>,
    action: Action,
) -> Result<Outcome> {
    let target = action.audit_target();
    let outcome = ceph_operations::execute(&connections, connection, revision, action).await;
    if let Some((action, resource)) = target {
        let result = match &outcome {
            Ok(result) if result.success => Ok(()),
            _ => Err(AppError::Storage("The RGW action did not succeed.".into())),
        };
        let _ = audit.record(connection, action, Some(resource), result);
    }
    outcome
}

#[tauri::command]
pub async fn ceph_index_batch(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    targets: Vec<BucketTarget>,
    on_result: Channel<IndexOutcome>,
) -> Result<()> {
    let session =
        connections.session_for(connection, Workspace::CephAdmin, IdentityKind::CephAdmin)?;
    ceph_operations::check_index_batch(&session, targets, |result| {
        on_result
            .send(result)
            .map_err(|_| AppError::Invalid("The diagnostics view closed.".into()))
    })
    .await
}

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
