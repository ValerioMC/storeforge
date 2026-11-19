use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryScope {
    Admin,
    Manager,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Success,
    Failure,
    Partial,
}

#[derive(Default, Deserialize)]
pub struct HistoryFilter {
    pub cursor: Option<u64>,
    pub context_id: Option<String>,
    pub search: Option<String>,
    pub actor: Option<String>,
    pub status: Option<EventStatus>,
}

#[derive(Serialize)]
pub struct HistoryQuery<'a> {
    pub limit: u32,
    pub cursor: Option<u64>,
    pub context_id: Option<&'a str>,
    pub search: Option<&'a str>,
    pub status: Option<EventStatus>,
}

#[derive(Deserialize, Serialize)]
pub struct ServiceAuditEvent {
    pub id: u64,
    pub created_at: DateTime<Utc>,
    pub user_email: String,
    pub action: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub context_name: Option<String>,
    pub status: EventStatus,
    pub user_role: Option<String>,
    pub scope: Option<String>,
    pub message: Option<String>,
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Deserialize, Serialize)]
pub struct ServiceAuditPage {
    pub logs: Vec<ServiceAuditEvent>,
    pub next_cursor: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct NotificationInbox {
    pub items: Vec<ServiceNotification>,
    pub unread_count: u64,
}

#[derive(Deserialize, Serialize)]
pub struct ServiceNotification {
    pub id: u64,
    #[serde(rename = "type")]
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Serialize)]
pub struct ReadNotifications {
    #[serde(default)]
    pub notification_ids: Vec<u64>,
    #[serde(default)]
    pub all: bool,
}

#[derive(Deserialize, Serialize)]
pub struct InboxCount {
    pub unread_count: u64,
}

#[derive(Deserialize, Serialize)]
pub struct QuotaPreferences {
    pub quota_alerts_enabled: bool,
    pub quota_alerts_global_watch: bool,
}

#[derive(Deserialize, Serialize)]
pub struct SmtpTest {
    pub recipient: String,
}

#[derive(Deserialize, Serialize)]
pub struct WebhookEndpoint {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub event_types: Vec<String>,
    pub has_signing_secret: bool,
    pub last_delivery: Option<WebhookDelivery>,
}

#[derive(Deserialize, Serialize)]
pub struct WebhookDelivery {
    pub id: u64,
    pub event_type: String,
    pub is_test: bool,
    pub status: WebhookStatus,
    pub attempt_count: u64,
    pub last_http_status: Option<u16>,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookStatus {
    Pending,
    Retrying,
    Delivered,
    Failed,
}

#[derive(Deserialize, Serialize)]
pub struct HealthSummary {
    pub endpoints: Vec<EndpointHealth>,
}

#[derive(Deserialize, Serialize)]
pub struct EndpointHealth {
    pub name: String,
    pub status: String,
    pub checked_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct HistorySummaryEnvelope {
    pub summary: HistorySummary,
}

#[derive(Deserialize, Serialize)]
pub struct HistorySummary {
    pub latest_collected_at: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct UsageEnvelope {
    pub aggregate: UsageSummary,
}

#[derive(Deserialize, Serialize)]
pub struct UsageSummary {
    pub newest_snapshot_at: Option<DateTime<Utc>>,
    pub warnings: Vec<String>,
    pub missing_bucket_count: u64,
    pub partial_scan_count: u64,
}

#[derive(Deserialize, Serialize)]
pub struct CostCollectionHistory {
    pub history: Vec<CostMonth>,
}

#[derive(Deserialize, Serialize)]
pub struct CostMonth {
    pub month: String,
    pub coverage: CostCoverage,
}

#[derive(Deserialize, Serialize)]
pub struct CostCoverage {
    pub days_collected: u32,
    pub days_in_month: u32,
}

#[derive(Deserialize, Serialize)]
pub struct HistoryCollectionResult {
    pub subjects_processed: u64,
    pub status: Option<String>,
    pub reason: Option<String>,
    pub errors: Vec<CollectionError>,
    pub warnings: Vec<CollectionWarning>,
}

#[derive(Deserialize, Serialize)]
pub struct CollectionError {
    pub subject_type: String,
    pub subject_id: u64,
    pub error: String,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum CollectionWarning {
    General(String),
    Subject {
        subject_type: String,
        subject_id: u64,
        warning: String,
    },
}
