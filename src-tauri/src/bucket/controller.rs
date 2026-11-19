use std::{fs::File, io::Read, sync::Arc};

use tauri::{State, WebviewWindow, ipc::Channel};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::{
    audit::service::AuditService,
    common::{
        document::save_text_document,
        error::{AppError, Result},
    },
    connection::{
        model::{IdentityKind, Workspace},
        service::ConnectionService,
    },
    monitoring::model::{BucketUsage, Reading},
    transfer::maintenance,
};

use super::{
    campaign::{self, ConfigurationUpdate},
    model::{
        Bucket, BucketCreation, BucketQuota, BucketQuotaUpdate, Configuration, ConfigurationChange,
    },
    quota, repository,
    service::BucketService,
    workbench::{self, BucketTarget, InventoryBucket, InventorySource},
};

#[tauri::command]
pub async fn list_buckets(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
) -> Result<Vec<Bucket>> {
    BucketService::list(&connections.bucket_session(connection)?).await
}

#[tauri::command]
pub async fn bucket_workbench_inventory(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    revision: Option<Uuid>,
    source: InventorySource,
    pages: Channel<Vec<InventoryBucket>>,
) -> Result<()> {
    let session = connections.session_for(connection, source.workspace(), source.identity())?;
    workbench::validate_revision(&session, revision)?;
    if source == InventorySource::StorageOps && !session.profile.settings.manager_enabled {
        return Err(AppError::Invalid(
            "Storage Ops is unavailable for this connection.".into(),
        ));
    }
    workbench::list(&session, source, |page| {
        pages
            .send(page)
            .map_err(|_| AppError::Invalid("The bucket inventory was closed.".into()))
    })
    .await
}

#[tauri::command]
pub async fn bucket_workbench_configuration(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    target: BucketTarget,
    configuration: Configuration,
    change: ConfigurationChange,
) -> Result<String> {
    let session = workbench_session(&connections, &target, IdentityKind::S3).await?;
    let write = !matches!(change, ConfigurationChange::Read);
    let result = repository::configure(&session, &target.name, configuration, change).await;
    if write {
        return audit.record(
            target.connection,
            "Change bucket settings",
            Some(target.name),
            result,
        );
    }
    result
}

#[tauri::command]
pub async fn apply_bucket_campaign_configuration(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    target: BucketTarget,
    update: ConfigurationUpdate,
) -> Result<String> {
    let session = workbench_session(&connections, &target, IdentityKind::S3).await?;
    let result = campaign::apply_configuration(&session, &target.name, update).await;
    audit.record(
        target.connection,
        "Apply bucket campaign configuration",
        Some(target.name),
        result,
    )
}

#[tauri::command]
pub async fn bucket_campaign_quota(
    connections: State<'_, Arc<ConnectionService>>,
    target: BucketTarget,
) -> Result<BucketQuota> {
    let session = workbench_session(&connections, &target, IdentityKind::CephAdmin).await?;
    quota::read(&session, &target.name).await
}

#[tauri::command]
pub async fn apply_bucket_campaign_quota(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    target: BucketTarget,
    update: BucketQuotaUpdate,
) -> Result<BucketQuota> {
    let session = workbench_session(&connections, &target, IdentityKind::CephAdmin).await?;
    let result = async {
        quota::write(&session, &target.name, update).await?;
        quota::read(&session, &target.name).await
    }
    .await;
    audit.record(
        target.connection,
        "Apply bucket campaign quota",
        Some(target.name),
        result,
    )
}

#[tauri::command]
pub async fn recalculate_bucket_campaign_usage(
    connections: State<'_, Arc<ConnectionService>>,
    target: BucketTarget,
) -> Result<Reading<BucketUsage>> {
    let session = workbench_session(&connections, &target, IdentityKind::S3).await?;
    Ok(campaign::recalculate_usage(&session, target.name).await)
}

async fn workbench_session(
    connections: &ConnectionService,
    target: &BucketTarget,
    identity: IdentityKind,
) -> Result<crate::connection::service::Session> {
    let session =
        connections.session_for(target.connection, target.source.workspace(), identity)?;
    workbench::validate_target(&session, target)?;
    if target.source == InventorySource::CephAdmin {
        let gateway = connections.session_for(
            target.connection,
            Workspace::CephAdmin,
            IdentityKind::CephAdmin,
        )?;
        workbench::validate_instance(&gateway, target).await?;
    }
    Ok(session)
}

#[tauri::command]
pub async fn create_bucket(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
    object_lock: bool,
    location: Option<String>,
    versioning: Option<bool>,
) -> Result<BucketCreation> {
    let result = BucketService::create(
        &connections.bucket_session(connection)?,
        &bucket,
        object_lock,
        location.as_deref(),
        versioning.unwrap_or_default(),
    )
    .await;
    let created = audit.record(connection, "Create bucket", Some(bucket.clone()), result)?;
    if let Some(error) = &created.versioning_error {
        let _ = audit.record::<()>(
            connection,
            "Enable bucket versioning",
            Some(bucket),
            Err(AppError::Storage(error.clone())),
        );
    }
    Ok(created)
}

#[tauri::command]
pub async fn finish_bucket_versioning(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
    location: Option<String>,
) -> Result<()> {
    let result = BucketService::finish_versioning(
        &connections.bucket_session(connection)?,
        &bucket,
        location.as_deref(),
    )
    .await;
    audit.record(connection, "Enable bucket versioning", Some(bucket), result)
}

#[tauri::command]
pub async fn delete_bucket(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
) -> Result<()> {
    let result =
        maintenance::delete_bucket_with_contents(&connections.bucket_session(connection)?, &bucket)
            .await;
    audit.record(connection, "Delete bucket", Some(bucket), result)
}

#[tauri::command]
pub async fn configure_bucket(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
    configuration: Configuration,
    change: ConfigurationChange,
) -> Result<String> {
    let write = !matches!(change, ConfigurationChange::Read);
    let result = repository::configure(
        &connections.bucket_session(connection)?,
        &bucket,
        configuration,
        change,
    )
    .await;
    if write {
        return audit.record(connection, "Change bucket settings", Some(bucket), result);
    }
    result
}

#[tauri::command]
pub async fn export_bucket_configuration(
    window: WebviewWindow,
    name: String,
    configuration: Option<Configuration>,
    document: String,
) -> Result<bool> {
    let (filter, extension) = match configuration {
        None | Some(Configuration::Policy) => ("JSON document", "json"),
        Some(_) => ("XML document", "xml"),
    };
    save_text_document(
        window,
        &name,
        "Export bucket configuration",
        filter,
        extension,
        document,
    )
    .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InventoryExport {
    Text,
    Csv,
    Json,
}

#[tauri::command]
pub async fn export_bucket_inventory(
    window: WebviewWindow,
    format: InventoryExport,
    document: String,
) -> Result<bool> {
    let (filter, extension) = match format {
        InventoryExport::Text => ("Text document", "txt"),
        InventoryExport::Csv => ("CSV document", "csv"),
        InventoryExport::Json => ("JSON document", "json"),
    };
    let name = format!("bucket-selection.{extension}");
    save_text_document(
        window,
        &name,
        "Export bucket selection",
        filter,
        extension,
        document,
    )
    .await
}

#[tauri::command]
pub async fn import_bucket_backup(window: WebviewWindow) -> Result<Option<String>> {
    let (send, receive) = oneshot::channel();
    window
        .dialog()
        .file()
        .set_title("Open bucket configuration backup")
        .set_parent(&window)
        .add_filter("StoreForge backup", &["json"])
        .pick_file(move |choice| {
            if send.send(choice).is_err() {
                tracing::debug!("Bucket backup picker was closed");
            }
        });
    let Some(choice) = receive
        .await
        .map_err(|error| AppError::File(error.to_string()))?
    else {
        return Ok(None);
    };
    let path = choice
        .into_path()
        .map_err(|error| AppError::File(error.to_string()))?;
    tokio::task::spawn_blocking(move || {
        const LIMIT: u64 = 32 * 1024 * 1024;
        let mut content = String::new();
        File::open(path)?
            .take(LIMIT + 1)
            .read_to_string(&mut content)?;
        if content.len() as u64 > LIMIT {
            return Err(AppError::Invalid("The backup exceeds 32 MiB.".into()));
        }
        Ok(Some(content))
    })
    .await
    .map_err(|error| AppError::File(error.to_string()))?
}

#[tauri::command]
pub async fn read_bucket_quota(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
) -> Result<BucketQuota> {
    let session =
        connections.session_for(connection, Workspace::CephAdmin, IdentityKind::CephAdmin)?;
    quota::read(&session, &bucket).await
}

#[tauri::command]
pub async fn update_bucket_quota(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
    update: BucketQuotaUpdate,
) -> Result<()> {
    let session =
        connections.session_for(connection, Workspace::CephAdmin, IdentityKind::CephAdmin)?;
    let result = quota::write(&session, &bucket, update).await;
    audit.record(connection, "Change bucket quota", Some(bucket), result)
}
