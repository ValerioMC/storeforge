use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::{
    audit::service::AuditService,
    common::error::Result,
    connection::model::{IdentityKind, Workspace},
    connection::service::ConnectionService,
    object::model::{Location, ObjectVersion},
};

use super::{
    batch,
    bucket_comparison::{
        self, BucketComparison, BucketPair, ComparisonRequest, ComparisonScope, Remediation,
    },
    inventory,
    maintenance::{self, IntegrityRequest, PurgePreview, PurgeRequest},
    model::{Comparison, Job, Transfer, Usage, VersionPlanItem, VersionRule},
    security::AccessInput,
    service::TransferService,
};

#[tauri::command]
pub fn list_jobs(transfers: State<'_, TransferService>) -> Result<Vec<Job>> {
    transfers.list()
}

#[tauri::command]
pub fn start_transfer(
    transfers: State<'_, TransferService>,
    transfer: Transfer,
    access: Option<AccessInput>,
) -> Result<Uuid> {
    transfers.start(transfer, access)
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
pub async fn compare_buckets(
    connections: State<'_, Arc<ConnectionService>>,
    source: Uuid,
    target: Uuid,
    request: ComparisonRequest,
) -> Result<BucketComparison> {
    let source = connections.session_for(source, Workspace::Manager, IdentityKind::S3)?;
    let target = connections.session_for(target, Workspace::Manager, IdentityKind::S3)?;
    bucket_comparison::compare(&source, &target, request).await
}

#[tauri::command]
pub async fn remediate_bucket_difference(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    scope: ComparisonScope,
    pair: BucketPair,
    action: Remediation,
) -> Result<()> {
    let (source, target) = scope.connections();
    let source = connections.session_for(source, Workspace::Manager, IdentityKind::S3)?;
    let destination = connections.session_for(target, Workspace::Manager, IdentityKind::S3)?;
    let resource = match &action {
        Remediation::Object { key, .. } => format!("{}/{}", pair.target, key),
        Remediation::Configuration { .. } => pair.target.clone(),
    };
    let result = bucket_comparison::apply(&source, &destination, scope, pair, action).await;
    audit.record(
        target,
        "Apply bucket comparison difference",
        Some(resource),
        result,
    )
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
pub fn check_integrity(
    transfers: State<'_, TransferService>,
    request: IntegrityRequest,
) -> Result<Uuid> {
    transfers.start(Transfer::Integrity { request }, None)
}

#[tauri::command]
pub async fn preview_bucket_purge(
    connections: State<'_, Arc<ConnectionService>>,
    request: PurgeRequest,
) -> Result<PurgePreview> {
    let session = connections.session_for(
        request.selection.connection,
        Workspace::Manager,
        IdentityKind::S3,
    )?;
    maintenance::preview_purge(&session, request).await
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
