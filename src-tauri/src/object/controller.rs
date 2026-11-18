use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::{
    audit::service::AuditService, common::error::Result, connection::service::ConnectionService,
};

use super::{
    model::{
        MultipartUpload, ObjectChange, ObjectDetails, ObjectId, ObjectPage, ObjectQuery, Preview,
        Tag, VersionCursor, VersionPage,
    },
    repository,
    service::ObjectService,
};

#[tauri::command]
pub async fn list_objects(
    connections: State<'_, Arc<ConnectionService>>,
    query: ObjectQuery,
) -> Result<ObjectPage> {
    repository::list(&connections.session(query.location.connection)?.s3, &query).await
}

#[tauri::command]
pub async fn list_versions(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
    prefix: String,
    exact: bool,
    cursor: Option<VersionCursor>,
) -> Result<VersionPage> {
    repository::versions(
        &connections.session(connection)?.s3,
        &bucket,
        &prefix,
        exact,
        cursor,
    )
    .await
}

#[tauri::command]
pub async fn object_details(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    object: ObjectId,
) -> Result<ObjectDetails> {
    ObjectService::details(&connections.session(connection)?.s3, &object).await
}

#[tauri::command]
pub async fn object_tags(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    object: ObjectId,
) -> Result<Vec<Tag>> {
    ObjectService::tags(&connections.session(connection)?.s3, &object).await
}

#[tauri::command]
pub async fn object_acl(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    object: ObjectId,
) -> Result<Vec<String>> {
    ObjectService::acl(&connections.session(connection)?.s3, &object).await
}

#[tauri::command]
pub async fn preview_object(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    object: ObjectId,
) -> Result<Preview> {
    ObjectService::preview(&connections.session(connection)?.s3, &object).await
}

#[tauri::command]
pub async fn share_object(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    object: ObjectId,
    minutes: u32,
) -> Result<String> {
    ObjectService::share_url(&connections.session(connection)?.s3, &object, minutes).await
}

#[tauri::command]
pub async fn create_folder(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
    key: String,
) -> Result<()> {
    let result =
        ObjectService::create_folder(&connections.session(connection)?.s3, &bucket, &key).await;
    audit.record(
        connection,
        "Create folder",
        Some(format!("{bucket}/{key}")),
        result,
    )
}

#[tauri::command]
pub async fn change_object(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    object: ObjectId,
    change: ObjectChange,
) -> Result<()> {
    let action = match &change {
        ObjectChange::Delete => "Delete object or version",
        ObjectChange::RestoreVersion => "Restore object version",
        ObjectChange::SetTags { .. } => "Change object tags",
        ObjectChange::SetMetadata { .. } => "Replace object metadata",
        ObjectChange::SetAcl { .. } => "Change object ACL",
        ObjectChange::LegalHold { .. } => "Change object legal hold",
        ObjectChange::Retention { .. } => "Change object retention",
        ObjectChange::RestoreArchive { .. } => "Request archive restore",
    };
    let result = ObjectService::change(&connections.session(connection)?.s3, &object, change).await;
    audit.record(
        connection,
        action,
        Some(format!("{}/{}", object.bucket, object.key)),
        result,
    )
}

#[tauri::command]
pub async fn list_multipart(
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
    prefix: String,
) -> Result<Vec<MultipartUpload>> {
    ObjectService::multipart(&connections.session(connection)?.s3, &bucket, &prefix).await
}

#[tauri::command]
pub async fn abort_multipart(
    audit: State<'_, AuditService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    bucket: String,
    key: String,
    upload_id: String,
) -> Result<()> {
    let result = ObjectService::abort_multipart(
        &connections.session(connection)?.s3,
        &bucket,
        &key,
        &upload_id,
    )
    .await;
    audit.record(
        connection,
        "Abort multipart upload",
        Some(format!("{bucket}/{key}")),
        result,
    )
}
