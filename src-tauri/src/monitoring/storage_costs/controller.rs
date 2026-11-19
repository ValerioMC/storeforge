use std::sync::Arc;

use chrono::NaiveDate;
use tauri::State;
use uuid::Uuid;

use crate::{
    administration::rgw_identities::RgwIdentityService, common::error::Result,
    connection::service::ConnectionService,
};

use super::{
    StorageCosts,
    model::{CollectionResult, Report, Settings, Summary},
};

#[tauri::command]
pub fn storage_cost_report(
    costs: State<'_, StorageCosts>,
    connections: State<'_, Arc<ConnectionService>>,
    month: NaiveDate,
) -> Result<Report> {
    costs.report(&connections, month)
}

#[tauri::command]
pub fn storage_cost_settings(
    costs: State<'_, StorageCosts>,
    connections: State<'_, Arc<ConnectionService>>,
    settings: Settings,
) -> Result<()> {
    costs.save(&connections, settings)
}

#[tauri::command]
pub async fn storage_cost_collect(
    costs: State<'_, StorageCosts>,
    connections: State<'_, Arc<ConnectionService>>,
    identities: State<'_, RgwIdentityService>,
    day: NaiveDate,
    endpoint: Option<Uuid>,
) -> Result<CollectionResult> {
    costs
        .collect(&connections, &identities, day, endpoint)
        .await
}

#[tauri::command]
pub async fn storage_cost_summary(
    costs: State<'_, StorageCosts>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    month: NaiveDate,
) -> Result<Option<Summary>> {
    costs.summary(&connections, connection, month).await
}

#[tauri::command]
pub async fn storage_cost_export(
    window: tauri::WebviewWindow,
    month: NaiveDate,
    csv: String,
) -> Result<bool> {
    super::service::report_range(month)?;
    crate::common::document::save_text_document(
        window,
        &format!("storage-costs-{}.csv", month.format("%Y-%m")),
        "Save storage costs",
        "CSV",
        "csv",
        csv,
    )
    .await
}
