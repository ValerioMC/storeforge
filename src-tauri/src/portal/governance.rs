use std::path::PathBuf;

use reqwest::{Method, header};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{PortalSession, ProjectSession, save_download};
use crate::{
    common::error::{AppError, Result},
    connection::shared_service::{
        model::ServiceSession, repository::SharedServiceRepository, service::SharedService,
    },
    portal::model::{
        AccessLogQuery, AdminRequestQuery, AdminResponse, GovernanceChange, GovernanceRead,
        LogExport, SettingChange,
    },
};

const ADMIN_REQUESTS: &str = "/api/admin/portal-requests";

pub(crate) async fn admin_requests(
    service: &SharedService,
    id: Uuid,
    query: AdminRequestQuery,
) -> Result<Value> {
    service
        .scoped(
            id,
            ServiceSession::admin_available,
            async |remote, origin, token| {
                remote
                    .get(origin, ADMIN_REQUESTS, &query, Some(token))
                    .await
            },
        )
        .await
}

pub(crate) async fn respond(
    service: &SharedService,
    id: Uuid,
    request: u64,
    response: AdminResponse,
) -> Result<Value> {
    let (action, draft) = match response {
        AdminResponse::Approve { message } => ("approve", json!({ "message": message })),
        AdminResponse::Reject { message } => ("reject", json!({ "message": message })),
        AdminResponse::Message { message } => ("messages", json!({ "message": message })),
    };
    service
        .scoped(
            id,
            ServiceSession::admin_available,
            async |remote, origin, token| {
                remote
                    .long_change(
                        Method::POST,
                        origin,
                        &format!("{ADMIN_REQUESTS}/{request}/{action}"),
                        token,
                        &draft,
                    )
                    .await
            },
        )
        .await
}

pub(crate) async fn export_logs(
    service: &SharedService,
    id: Uuid,
    project: u64,
    query: LogExport,
    destination: PathBuf,
) -> Result<()> {
    if query.date_from > query.date_to {
        return Err(AppError::Invalid(
            "The end date must follow the start date.".into(),
        ));
    }
    let temporary = service
        .scoped(
            id,
            |session| session.access.portal.available,
            async |remote, origin, token| {
                let portal = ProjectSession::new(
                    PortalSession {
                        remote,
                        origin,
                        token,
                    },
                    project,
                    None,
                )?;
                let request = portal
                    .portal
                    .request(Method::GET, "/api/portal/access-logs/raw")?
                    .query(&[("account_id", &portal.account)])
                    .query(&query)
                    .timeout(std::time::Duration::from_secs(600));
                let response = SharedServiceRepository::send(request).await?;
                if !response
                    .headers()
                    .get(header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.split(';').next() == Some("text/plain"))
                {
                    return Err(AppError::Storage(
                        "The service did not return provider access logs.".into(),
                    ));
                }
                save_download(response, &destination).await
            },
        )
        .await?;
    temporary
        .persist(destination)
        .map_err(|failure| AppError::from(failure.error))?;
    Ok(())
}

impl ProjectSession<'_> {
    pub(super) async fn read_governance(&self, view: GovernanceRead) -> Result<Value> {
        match view {
            GovernanceRead::Settings => self.get("/api/portal/settings", &[]).await,
            GovernanceRead::History { space_id } => {
                let mut query = vec![("limit", "200")];
                if let Some(space) = &space_id {
                    query.push(("space_id", space))
                }
                self.get("/api/portal/activity", &query).await
            }
            GovernanceRead::AccessLogs { query } => self.access_logs(query).await,
            GovernanceRead::Composition => self.get("/api/portal/usage-stats/latest", &[]).await,
            GovernanceRead::Traffic { window } => {
                let window = match window {
                    crate::portal::model::TrafficWindow::Hour => "hour",
                    crate::portal::model::TrafficWindow::Day => "day",
                    crate::portal::model::TrafficWindow::Week => "week",
                    crate::portal::model::TrafficWindow::Month => "month",
                };
                self.get("/api/portal/traffic", &[("window", window)]).await
            }
            GovernanceRead::Cost { month } => {
                self.get("/api/portal/cost/me", &[("month", &month)]).await
            }
        }
    }

    pub(super) async fn change_governance(&self, action: GovernanceChange) -> Result<Value> {
        match action {
            GovernanceChange::SaveSetting { setting } => {
                let (key, value) = setting_parts(setting)?;
                let draft = match key.split_once('.') {
                    Some((group, field)) => json!({ group: { field: value } }),
                    None => json!({ key: value }),
                };
                self.submit(Method::PATCH, "/api/portal/settings", &draft)
                    .await
            }
            GovernanceChange::RequestSetting { setting, reason } => {
                let (key, value) = setting_parts(setting)?;
                if key == "cost_estimation_enabled" {
                    return Err(AppError::Invalid(
                        "Cost estimation is managed through project settings.".into(),
                    ));
                }
                let mode = if value.is_null() {
                    "inherit"
                } else {
                    "override"
                };
                self.submit(Method::POST, "/api/portal/requests", &json!({
                    "request_type": "portal_setting_change", "setting": key, "mode": mode, "value": value, "reason": reason,
                })).await
            }
            GovernanceChange::RequestQuota {
                direction,
                size,
                unit,
                reason,
            } => {
                if !size.is_finite() || size <= 0.0 {
                    return Err(AppError::Invalid("Enter a positive target quota.".into()));
                }
                self.submit(Method::POST, "/api/portal/requests", &json!({
                    "request_type": "account_quota_change", "direction": direction, "target_quota_value": size,
                    "target_quota_unit": unit, "reason": reason,
                })).await
            }
        }
    }

    async fn access_logs(&self, query: AccessLogQuery) -> Result<Value> {
        let date = query.date.to_string();
        let offset = query.offset.to_string();
        let timezone = query.timezone_offset_minutes.to_string();
        let mut parameters = vec![
            ("date", date.as_str()),
            ("offset", offset.as_str()),
            ("limit", "100"),
            ("timezone_offset_minutes", timezone.as_str()),
        ];
        if let Some(space) = &query.space_id {
            parameters.push(("space_id", space))
        }
        let filter = query.filter.map(|rule| {
            json!({
                "match":"all", "rules":[{"field":rule.field,"op":"contains","value":rule.value}],
            })
            .to_string()
        });
        if let Some(filter) = &filter {
            parameters.push(("advanced_filter", filter))
        }
        self.get("/api/portal/access-logs/page", &parameters).await
    }
}

fn setting_parts(setting: SettingChange) -> Result<(String, Value)> {
    #[derive(serde::Deserialize)]
    struct SettingProjection {
        key: String,
        value: Value,
    }
    let payload: SettingProjection = serde_json::from_value(serde_json::to_value(setting)?)?;
    Ok((payload.key, payload.value))
}
