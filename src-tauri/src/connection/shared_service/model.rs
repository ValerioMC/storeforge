use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::common::error::{AppError, Result};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProfile {
    pub id: Uuid,
    #[serde(flatten)]
    pub settings: ServiceDraft,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDraft {
    pub name: String,
    pub origin: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub profile: ServiceProfile,
    pub session: Option<ServiceSession>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSession {
    pub principal: ServicePrincipal,
    pub access: WorkspaceAccess,
    pub expires_at: DateTime<Utc>,
}

impl ServiceSession {
    pub fn admin_available(&self) -> bool {
        self.access.admin.available && self.principal.role.is_admin()
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ServicePrincipal {
    pub id: i64,
    pub email: String,
    pub role: ServiceRole,
    #[serde(rename(deserialize = "is_active", serialize = "active"))]
    pub active: bool,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceRole {
    #[serde(rename = "ui_admin")]
    Admin,
    #[serde(rename = "ui_superadmin")]
    Superadmin,
    #[serde(rename = "ui_user")]
    User,
}

impl ServiceRole {
    pub fn is_admin(self) -> bool {
        matches!(self, Self::Admin | Self::Superadmin)
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct WorkspaceAccess {
    pub admin: WorkspaceAvailability,
    pub manager: WorkspaceAvailability,
    pub browser: WorkspaceAvailability,
    pub ceph_admin: WorkspaceAvailability,
    pub storage_ops: WorkspaceAvailability,
    pub portal: WorkspaceAvailability,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct WorkspaceAvailability {
    pub available: bool,
    pub context_count: u64,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SharedWorkspace {
    Manager,
    Browser,
}

impl SharedWorkspace {
    pub fn available(self, access: &WorkspaceAccess) -> bool {
        match self {
            Self::Manager => access.manager.available,
            Self::Browser => access.browser.available,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct SharedContext {
    pub id: String,
    pub kind: ContextKind,
    #[serde(rename(deserialize = "display_name"))]
    pub name: String,
    pub connection_scope: Option<ConnectionScope>,
    pub manager_role: Option<String>,
    pub portal_role: Option<String>,
    pub endpoint_name: String,
    pub endpoint_url: String,
    pub capabilities: ContextCapabilities,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextKind {
    Account,
    Connection,
    S3User,
    PortalAccount,
    OpenstackProject,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionScope {
    Shared,
    Private,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct ContextCapabilities {
    pub can_manage_iam: bool,
    pub sts_capable: bool,
    pub admin_api_capable: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InteractiveAction {
    AccountProfile,
    AccountSecurity,
    PrivateConnections,
    IdentitySecurity,
    ApiTokens,
    AuthenticationSettings,
    AdminUsers,
    AdminGroups,
    SharedConnections,
    AccessReview,
    CostSettings,
    NewWebhook,
    ProviderAccessLogs,
    EditWebhook { id: u64 },
    StorageEndpointSettings,
    ReviewUser { id: i64, search: String },
    ReviewGroup { id: i64, search: String },
}

impl InteractiveAction {
    pub fn url(&self, origin: &Url) -> Result<Url> {
        let path = match self {
            Self::AccountProfile => "/profile?tab=profile",
            Self::AccountSecurity => "/profile?tab=security",
            Self::PrivateConnections => "/profile?tab=connections",
            Self::IdentitySecurity => "/admin/identity-security",
            Self::ApiTokens => "/admin/profile?tab=api-tokens",
            Self::AuthenticationSettings => "/admin/authentication-settings",
            Self::AdminUsers | Self::ReviewUser { .. } => "/admin/users",
            Self::AdminGroups | Self::ReviewGroup { .. } => "/admin/groups",
            Self::SharedConnections => "/admin/s3-connections",
            Self::AccessReview => "/admin/access-audit",
            Self::CostSettings => "/admin/cost-settings",
            Self::NewWebhook => "/admin/webhook-settings/new",
            Self::ProviderAccessLogs => "/portal/history?view=access",
            Self::EditWebhook { .. } => "/admin/webhook-settings/",
            Self::StorageEndpointSettings => "/admin/storage-endpoints",
        };
        let mut url = origin
            .join(path)
            .map_err(|_| AppError::Invalid("Invalid browser action.".into()))?;
        if let Self::EditWebhook { id } = self {
            if *id == 0 {
                return Err(AppError::Invalid("Select a webhook to edit.".into()));
            }
            url = url
                .join(&id.to_string())
                .map_err(|_| AppError::Invalid("Invalid webhook action.".into()))?;
        }
        if let Self::ReviewUser { id, search } | Self::ReviewGroup { id, search } = self {
            if *id <= 0 {
                return Err(AppError::Invalid("Select a principal to review.".into()));
            }
            url.query_pairs_mut()
                .append_pair("edit", &id.to_string())
                .append_pair("search", search);
        }
        Ok(url)
    }

    pub fn admin_only(&self) -> bool {
        !matches!(
            self,
            Self::AccountProfile
                | Self::AccountSecurity
                | Self::PrivateConnections
                | Self::ProviderAccessLogs
        )
    }
}
