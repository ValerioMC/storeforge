use tauri::{State, WebviewWindow};
use uuid::Uuid;

use crate::{
    common::{document::save_text_document, error::Result},
    connection::shared_service::service::SharedService,
};

use super::{
    model::{
        AccessFilters, AccessRow, GovernancePage, Group, GroupDraft, ListingQuery,
        ManagedConnection, PersonalConnection,
    },
    service::GovernanceService,
};

#[tauri::command]
pub async fn list_platform_access(
    shared: State<'_, SharedService>,
    id: Uuid,
    page: u32,
    filters: AccessFilters,
) -> Result<GovernancePage<AccessRow>> {
    GovernanceService::new(&shared, id)
        .access(page, &filters)
        .await
}

#[tauri::command]
pub async fn export_platform_access(
    window: WebviewWindow,
    shared: State<'_, SharedService>,
    id: Uuid,
    filters: AccessFilters,
) -> Result<bool> {
    let csv = GovernanceService::new(&shared, id)
        .export_access(&filters)
        .await?;
    save_text_document(
        window,
        "access-review.csv",
        "Export access review",
        "CSV document",
        "csv",
        csv,
    )
    .await
}

#[tauri::command]
pub async fn list_platform_groups(
    shared: State<'_, SharedService>,
    id: Uuid,
    query: ListingQuery,
) -> Result<GovernancePage<Group>> {
    GovernanceService::new(&shared, id).groups(&query).await
}

#[tauri::command]
pub async fn list_platform_connections(
    shared: State<'_, SharedService>,
    id: Uuid,
    query: ListingQuery,
) -> Result<GovernancePage<ManagedConnection>> {
    GovernanceService::new(&shared, id)
        .managed_connections(&query)
        .await
}

#[tauri::command]
pub async fn list_private_platform_connections(
    shared: State<'_, SharedService>,
    id: Uuid,
) -> Result<Vec<PersonalConnection>> {
    GovernanceService::new(&shared, id)
        .private_connections()
        .await
}

#[tauri::command]
pub async fn save_platform_group(
    shared: State<'_, SharedService>,
    id: Uuid,
    group_id: Option<i64>,
    draft: GroupDraft,
) -> Result<Group> {
    GovernanceService::new(&shared, id)
        .save_group(group_id, draft)
        .await
}

#[tauri::command]
pub async fn rename_platform_connection(
    shared: State<'_, SharedService>,
    id: Uuid,
    connection_id: i64,
    name: String,
) -> Result<ManagedConnection> {
    GovernanceService::new(&shared, id)
        .rename_connection(connection_id, &name)
        .await
}
