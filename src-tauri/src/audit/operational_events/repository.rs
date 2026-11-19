use reqwest::Method;
use serde::{Serialize, de::DeserializeOwned};
use url::Url;
use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::shared_service::{
        model::{ServiceSession, SharedWorkspace},
        repository::SharedServiceRepository,
        service::SharedService,
    },
};

use super::model::{
    CostCollectionHistory, HealthSummary, HistoryCollectionResult, HistoryQuery, HistoryScope,
    HistorySummaryEnvelope, InboxCount, NotificationInbox, QuotaPreferences, ReadNotifications,
    ServiceAuditEvent, ServiceAuditPage, SmtpTest, UsageEnvelope, WebhookDelivery, WebhookEndpoint,
};

const EMPTY_QUERY: &[(&str, &str)] = &[];

pub struct OperationsRepository<'a> {
    shared: &'a SharedService,
    id: Uuid,
}

impl<'a> OperationsRepository<'a> {
    pub fn new(shared: &'a SharedService, id: Uuid) -> Self {
        Self { shared, id }
    }

    pub async fn history<T>(
        &self,
        scope: HistoryScope,
        read: impl AsyncFnOnce(HistoryReader<'_>) -> Result<T>,
    ) -> Result<T> {
        self.shared
            .scoped(
                self.id,
                |session| match scope {
                    HistoryScope::Admin => session.admin_available(),
                    HistoryScope::Manager => SharedWorkspace::Manager.available(&session.access),
                },
                async |remote, origin, token| {
                    read(HistoryReader {
                        remote,
                        origin,
                        token,
                        scope,
                    })
                    .await
                },
            )
            .await
    }

    pub async fn inbox(&self) -> Result<NotificationInbox> {
        self.profile("/api/users/me/notifications", &[("limit", 100)])
            .await
    }

    pub async fn mark_read(&self, change: &ReadNotifications) -> Result<InboxCount> {
        self.shared
            .scoped(
                self.id,
                |_| true,
                async |remote, origin, token| {
                    remote
                        .change(
                            Method::POST,
                            origin,
                            "/api/users/me/notifications/read",
                            token,
                            change,
                        )
                        .await
                },
            )
            .await
    }

    pub async fn quota_preferences(
        &self,
        draft: Option<&QuotaPreferences>,
    ) -> Result<QuotaPreferences> {
        let path = "/api/users/me";
        match draft {
            Some(draft) => {
                self.shared
                    .scoped(
                        self.id,
                        |_| true,
                        async |remote, origin, token| {
                            remote
                                .change(Method::PATCH, origin, path, token, draft)
                                .await
                        },
                    )
                    .await
            }
            None => self.profile(path, EMPTY_QUERY).await,
        }
    }

    pub async fn test_smtp(&self) -> Result<SmtpTest> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    let settings: serde_json::Value = remote
                        .get(origin, "/api/admin/settings", EMPTY_QUERY, Some(token))
                        .await?;
                    let draft = settings
                        .get("quota_notifications")
                        .filter(|entry| entry.is_object())
                        .ok_or_else(|| {
                            AppError::Storage(
                                "The service did not publish quota notification settings.".into(),
                            )
                        })?;
                    remote
                        .change(
                            Method::POST,
                            origin,
                            "/api/admin/settings/quota-notifications/test-email",
                            token,
                            draft,
                        )
                        .await
                },
            )
            .await
    }

    pub async fn webhooks(&self) -> Result<Vec<WebhookEndpoint>> {
        self.admin("/api/admin/settings/webhooks", EMPTY_QUERY)
            .await
    }

    pub async fn deliveries(&self, endpoint: u64) -> Result<Vec<WebhookDelivery>> {
        self.admin(
            &format!("/api/admin/settings/webhooks/{endpoint}/deliveries"),
            &[("limit", 100)],
        )
        .await
    }

    pub async fn health(&self) -> Result<HealthSummary> {
        self.admin("/api/admin/health/summary", EMPTY_QUERY).await
    }

    pub async fn usage_history(&self) -> Result<HistorySummaryEnvelope> {
        self.admin(
            "/api/admin/usage-history",
            &[("page_size", "1"), ("granularity", "hourly")],
        )
        .await
    }

    pub async fn bucket_usage(&self, endpoint: u64) -> Result<UsageEnvelope> {
        self.admin(
            "/api/admin/usage-stats/latest",
            &[("endpoint_id", endpoint)],
        )
        .await
    }

    pub async fn cost_history(&self, month: &str) -> Result<CostCollectionHistory> {
        self.admin("/api/admin/cost/analytics", &[("month", month)])
            .await
    }

    pub async fn collect_history(&self) -> Result<HistoryCollectionResult> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    remote
                        .long_change(
                            Method::POST,
                            origin,
                            "/api/admin/usage-history/collect",
                            token,
                            &(),
                        )
                        .await
                },
            )
            .await
    }

    async fn admin<T: DeserializeOwned, Q: Serialize + ?Sized>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| remote.get(origin, path, query, Some(token)).await,
            )
            .await
    }

    async fn profile<T: DeserializeOwned, Q: Serialize + ?Sized>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T> {
        self.shared
            .scoped(
                self.id,
                |_| true,
                async |remote, origin, token| remote.get(origin, path, query, Some(token)).await,
            )
            .await
    }
}

pub struct HistoryReader<'a> {
    remote: &'a SharedServiceRepository,
    origin: &'a Url,
    token: &'a str,
    scope: HistoryScope,
}

impl HistoryReader<'_> {
    pub async fn page(&self, query: &HistoryQuery<'_>) -> Result<ServiceAuditPage> {
        if self.scope == HistoryScope::Admin {
            return self
                .remote
                .get(
                    self.origin,
                    "/api/admin/audit/logs",
                    query,
                    Some(self.token),
                )
                .await;
        }
        let logs: Vec<ServiceAuditEvent> = self
            .remote
            .get(
                self.origin,
                "/api/manager/activity",
                query,
                Some(self.token),
            )
            .await?;
        let next_cursor = logs
            .last()
            .filter(|_| logs.len() == query.limit as usize)
            .map(|event| event.id);
        Ok(ServiceAuditPage { logs, next_cursor })
    }
}
