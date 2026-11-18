use std::sync::Arc;

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
    connection::service::ConnectionService,
};

use super::{
    model::{Bucket, Configuration, ConfigurationChange},
    repository,
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
) -> Result<()> {
    let result =
        BucketService::create(&connections.session(connection)?, &bucket, object_lock).await;
    audit.record(connection, "Create bucket", Some(bucket), result)
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
    configuration: Configuration,
    document: String,
) -> Result<bool> {
    let (filter, extension) = match configuration {
        Configuration::Policy => ("JSON document", "json"),
        _ => ("XML document", "xml"),
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
