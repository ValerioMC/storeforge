use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::{common::error::Result, connection::shared_service::service::SharedService};

use super::{
    model::{
        CostCollectionHistory, HealthSummary, HistoryCollectionResult, HistoryFilter, HistoryScope,
        HistorySummaryEnvelope, InboxCount, NotificationInbox, QuotaPreferences, ReadNotifications,
        ServiceAuditPage, SmtpTest, UsageEnvelope, WebhookDelivery, WebhookEndpoint,
    },
    repository::OperationsRepository,
    service::OperationsService,
};

#[tauri::command]
pub async fn operational_events(
    shared: State<'_, SharedService>,
    id: Uuid,
    scope: HistoryScope,
    filter: HistoryFilter,
) -> Result<ServiceAuditPage> {
    OperationsService::new(&shared, id)
        .history(scope, filter)
        .await
}

#[tauri::command]
pub async fn operational_inbox(
    shared: State<'_, SharedService>,
    id: Uuid,
) -> Result<NotificationInbox> {
    OperationsRepository::new(&shared, id).inbox().await
}

#[tauri::command]
pub async fn read_operational_notifications(
    shared: State<'_, SharedService>,
    id: Uuid,
    change: ReadNotifications,
) -> Result<InboxCount> {
    OperationsRepository::new(&shared, id)
        .mark_read(&change)
        .await
}

#[tauri::command]
pub async fn operational_quota_preferences(
    shared: State<'_, SharedService>,
    id: Uuid,
    draft: Option<QuotaPreferences>,
) -> Result<QuotaPreferences> {
    OperationsRepository::new(&shared, id)
        .quota_preferences(draft.as_ref())
        .await
}

#[tauri::command]
pub async fn test_operational_smtp(shared: State<'_, SharedService>, id: Uuid) -> Result<SmtpTest> {
    OperationsRepository::new(&shared, id).test_smtp().await
}

#[tauri::command]
pub async fn operational_webhooks(
    shared: State<'_, SharedService>,
    id: Uuid,
) -> Result<Vec<WebhookEndpoint>> {
    OperationsRepository::new(&shared, id).webhooks().await
}

#[tauri::command]
pub async fn operational_webhook_deliveries(
    shared: State<'_, SharedService>,
    id: Uuid,
    endpoint: u64,
) -> Result<Vec<WebhookDelivery>> {
    OperationsRepository::new(&shared, id)
        .deliveries(endpoint)
        .await
}

#[tauri::command]
pub async fn operational_health(
    shared: State<'_, SharedService>,
    id: Uuid,
) -> Result<HealthSummary> {
    OperationsRepository::new(&shared, id).health().await
}

#[tauri::command]
pub async fn operational_usage_history(
    shared: State<'_, SharedService>,
    id: Uuid,
) -> Result<HistorySummaryEnvelope> {
    OperationsRepository::new(&shared, id).usage_history().await
}

#[tauri::command]
pub async fn operational_bucket_usage(
    shared: State<'_, SharedService>,
    id: Uuid,
    endpoint: u64,
) -> Result<UsageEnvelope> {
    OperationsRepository::new(&shared, id)
        .bucket_usage(endpoint)
        .await
}

#[tauri::command]
pub async fn operational_cost_history(
    shared: State<'_, SharedService>,
    id: Uuid,
) -> Result<CostCollectionHistory> {
    let month = Utc::now().format("%Y-%m").to_string();
    OperationsRepository::new(&shared, id)
        .cost_history(&month)
        .await
}

#[tauri::command]
pub async fn collect_operational_usage_history(
    shared: State<'_, SharedService>,
    id: Uuid,
) -> Result<HistoryCollectionResult> {
    OperationsRepository::new(&shared, id)
        .collect_history()
        .await
}
