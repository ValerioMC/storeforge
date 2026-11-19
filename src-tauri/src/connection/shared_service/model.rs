use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    UiAdmin,
    UiSuperadmin,
    UiUser,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct WorkspaceAccess {
    pub admin: WorkspaceAvailability,
    pub manager: WorkspaceAvailability,
    pub browser: WorkspaceAvailability,
    pub ceph_admin: WorkspaceAvailability,
    pub storage_ops: WorkspaceAvailability,
    pub portal: WorkspaceAvailability,
}

#[derive(Clone, Deserialize, Serialize)]
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
    AccountSecurity,
    IdentitySecurity,
}

impl InteractiveAction {
    pub fn path(&self) -> &'static str {
        match self {
            Self::AccountSecurity => "/profile?tab=security",
            Self::IdentitySecurity => "/admin/identity-security",
        }
    }

    pub fn admin_only(&self) -> bool {
        matches!(self, Self::IdentitySecurity)
    }
}
