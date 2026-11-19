use std::{fs::File, io::Read, sync::Arc};

use tauri::{State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::{
    audit::service::AuditService,
    common::{
        document::write_text,
        error::{AppError, Result},
    },
    connection::{
        model::{IdentityKind, Workspace},
        service::ConnectionService,
    },
};

use super::{
    model::{
        Bucket, BucketCreation, BucketQuota, BucketQuotaUpdate, Configuration, ConfigurationChange,
    },
    quota, repository,
    service::BucketService,
};

#[tauri::command]
pub async fn list_buckets(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
) -> Result<Vec<Bucket>> {
    BucketService::list(&connections.session(connection)?).await
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
        &connections.session(connection)?,
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
        &connections.session(connection)?,
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
    let result = BucketService::delete(&connections.session(connection)?, &bucket).await;
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
        &connections.session(connection)?,
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
    let (send, receive) = oneshot::channel();
    window
        .dialog()
        .file()
        .set_title("Export bucket configuration")
        .set_parent(&window)
        .set_file_name(name)
        .add_filter(filter, &[extension])
        .save_file(move |choice| {
            if send.send(choice).is_err() {
                tracing::debug!("Bucket configuration export was cancelled");
            }
        });
    let Some(choice) = receive
        .await
        .map_err(|error| AppError::File(error.to_string()))?
    else {
        return Ok(false);
    };
    let path = choice
        .into_path()
        .map_err(|error| AppError::File(error.to_string()))?;
    tokio::task::spawn_blocking(move || write_text(&path, &document))
        .await
        .map_err(|error| AppError::File(error.to_string()))??;
    Ok(true)
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
