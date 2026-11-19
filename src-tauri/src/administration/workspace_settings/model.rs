use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SettingsSection {
    General,
    Manager,
    Browser,
    Portal,
    QuotaNotifications,
}

impl SettingsSection {
    pub fn key(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Manager => "manager",
            Self::Browser => "browser",
            Self::Portal => "portal",
            Self::QuotaNotifications => "quota_notifications",
        }
    }
}

pub type ScalarSettings = BTreeMap<String, SettingValue>;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum SettingValue {
    Switch(bool),
    Integer(u64),
    Policy(FeaturePolicy),
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FeaturePolicy {
    Disabled,
    Targeted,
    DefaultOn,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct FeatureLock {
    pub forced: bool,
    pub source: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct SettingGroup {
    pub name: Option<String>,
    pub values: ScalarSettings,
}

#[derive(Deserialize)]
pub struct SettingEdit {
    pub group: Option<String>,
    pub field: String,
    pub before: SettingValue,
    pub value: SettingValue,
}

#[derive(Serialize)]
pub struct WorkspaceSettings {
    pub groups: Vec<SettingGroup>,
    pub locks: BTreeMap<String, FeatureLock>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AccountPolicy {
    BrowserAccessEnabled,
    CostEstimationEnabled,
    AllowPrivateStorageSpaceCreate,
    AllowPortalNamedBucketCreate,
    AllowPortalUserAccessKeyCreate,
    AllowPortalUserExternalSharing,
    ServerAccessLoggingEnabled,
    StorageSpaceVersionCleanupEnabled,
}

#[derive(Deserialize, Serialize)]
pub struct AccountOverride {
    pub policy: AccountPolicy,
    pub value: Option<bool>,
}

#[derive(Deserialize, Serialize)]
pub struct AccountPortalSettings {
    pub admin_override: BTreeMap<String, PortalFlag>,
    pub effective: BTreeMap<String, PortalFlag>,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum PortalFlag {
    Switch(bool),
    Other(serde_json::Value),
}

#[derive(Deserialize, Serialize)]
pub struct PolicyAccount {
    pub id: u64,
    pub name: String,
}

#[derive(Deserialize, Serialize)]
pub struct PolicyAccounts {
    pub items: Vec<PolicyAccount>,
    pub has_next: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Readiness {
    pub environment: String,
    pub profile: String,
    pub status: String,
    pub findings: Vec<ReadinessFinding>,
}

#[derive(Deserialize, Serialize)]
pub struct ReadinessFinding {
    pub code: String,
    pub label: String,
    pub result: String,
    pub level: String,
    pub severity: Option<String>,
    pub message: String,
    pub documentation_url: String,
    pub blocks_startup: bool,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RotationKind {
    EndpointAdmin,
    EndpointRuntime,
    EndpointSupervision,
    CephAdmin,
    Account,
    S3User,
}

impl RotationKind {
    pub fn requires_delete(self) -> bool {
        matches!(self, Self::EndpointRuntime | Self::EndpointSupervision)
    }

    pub fn identity(self) -> Option<&'static str> {
        match self {
            Self::EndpointRuntime => Some("runtime"),
            Self::EndpointSupervision => Some("supervision"),
            Self::CephAdmin => Some("ceph_admin"),
            _ => None,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct RotationEndpoint {
    pub id: u64,
    pub name: String,
    pub provider: String,
    pub is_editable: bool,
    pub capabilities: BTreeMap<String, bool>,
    pub service_identities: Vec<RotationIdentity>,
}

#[derive(Deserialize, Serialize)]
pub struct RotationIdentity {
    pub kind: String,
    pub mode: String,
    pub status: String,
    pub rotation_phase: Option<RotationPhase>,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RotationPhase {
    Prepared,
    Activated,
}

#[derive(Deserialize, Serialize)]
pub struct RotationPlan {
    pub endpoint_ids: Vec<u64>,
    pub key_types: Vec<RotationKind>,
    pub deactivate_only: bool,
}

#[derive(Deserialize, Serialize)]
pub struct RotationRecovery {
    pub endpoint_id: u64,
    pub kind: RotationKind,
    pub phase: RotationPhase,
    pub deactivate_only: bool,
}

#[derive(Deserialize, Serialize)]
pub struct RotationReport {
    pub summary: RotationSummary,
    pub results: Vec<RotationResult>,
}

#[derive(Deserialize, Serialize)]
pub struct RotationSummary {
    pub rotated: u64,
    pub failed: u64,
    pub skipped: u64,
    pub deleted_old_keys: u64,
    pub disabled_old_keys: u64,
}

#[derive(Deserialize, Serialize)]
pub struct RotationResult {
    pub endpoint_id: u64,
    pub endpoint_name: String,
    pub key_type: RotationKind,
    pub target_id: Option<String>,
    pub target_label: Option<String>,
    pub status: RotationStatus,
    pub message: Option<String>,
    pub rotation_phase: Option<RotationPhase>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationStatus {
    Rotated,
    Failed,
    Skipped,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Account,
    User,
    Connection,
}

impl ResourceKind {
    pub fn path(self) -> &'static str {
        match self {
            Self::Account => "/api/admin/accounts",
            Self::User => "/api/admin/s3-users",
            Self::Connection => "/api/admin/s3-connections",
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePolicy {
    #[serde(rename = "allow_activity_history")]
    ActivityHistory,
    #[serde(rename = "allow_bucket_quota_management")]
    BucketQuotaManagement,
    #[serde(rename = "allow_access_key_management")]
    AccessKeyManagement,
    #[serde(rename = "allow_access_key_metadata")]
    AccessKeyMetadata,
    #[serde(rename = "allow_access_key_expiration")]
    AccessKeyExpiration,
    #[serde(rename = "allow_rgw_usage_metrics")]
    RgwUsageMetrics,
    #[serde(rename = "allow_cost_estimation")]
    CostEstimation,
    #[serde(rename = "allow_bucket_usage_stats")]
    BucketUsageStats,
    #[serde(rename = "allow_bucket_usage_stats_collection")]
    BucketUsageStatsCollection,
    #[serde(rename = "allow_multisite_endpoint_switching")]
    MultisiteEndpointSwitching,
    #[serde(rename = "allow_manager_browser_data_access_for_all_admins")]
    ManagerBrowserDataAccessForAllAdmins,
    #[serde(rename = "allow_managed_private_connection_provisioning")]
    ManagedPrivateConnectionProvisioning,
}

impl ResourcePolicy {
    pub fn available(&self, kind: ResourceKind) -> bool {
        match kind {
            ResourceKind::Connection => matches!(self, Self::ActivityHistory),
            ResourceKind::User => true,
            ResourceKind::Account => !matches!(
                self,
                Self::AccessKeyManagement | Self::ManagedPrivateConnectionProvisioning
            ),
        }
    }
}

#[derive(Deserialize)]
pub struct ResourceOverride {
    pub policy: ResourcePolicy,
    pub value: Option<bool>,
}

#[derive(Serialize)]
pub struct ResourcePolicies {
    pub items: Vec<ResourcePolicyItem>,
    pub has_next: bool,
}

#[derive(Serialize)]
pub struct ResourcePolicyItem {
    pub id: u64,
    pub name: String,
    pub overrides: BTreeMap<ResourcePolicy, Option<bool>>,
}

#[derive(Deserialize)]
pub struct ResourcePagePayload {
    pub items: Vec<ResourcePayload>,
    pub has_next: bool,
}

#[derive(Deserialize)]
pub struct ResourcePayload {
    pub id: u64,
    pub name: String,
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize)]
pub struct ServiceCostRates {
    pub storage_gb_month_price: String,
    pub egress_gb_price: String,
    pub ingress_gb_price: String,
    pub requests_per_1000_price: String,
}

#[derive(Deserialize, Serialize)]
pub struct ServiceCostModel {
    pub currency: String,
    #[serde(flatten)]
    pub rates: ServiceCostRates,
}

#[derive(Deserialize, Serialize)]
pub struct ServiceCostOverride {
    pub storage_endpoint_id: u64,
    #[serde(flatten)]
    pub rates: ServiceCostRates,
}

#[derive(Deserialize, Serialize)]
pub struct ServiceCostSettings {
    pub model: Option<ServiceCostModel>,
    pub endpoint_overrides: Vec<ServiceCostOverride>,
}
