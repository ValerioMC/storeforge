use tauri::State;
use uuid::Uuid;

use crate::{common::error::Result, connection::shared_service::service::SharedService};

use super::{
    model::{
        AccountOverride, AccountPortalSettings, PolicyAccounts, Readiness, ResourceKind,
        ResourceOverride, ResourcePolicies, ResourcePolicyItem, RotationEndpoint, RotationPlan,
        RotationRecovery, RotationReport, ServiceCostSettings, SettingEdit, SettingsSection,
        WorkspaceSettings,
    },
    service::WorkspaceSettingsService,
};

#[tauri::command]
pub async fn workspace_policy_settings(
    shared: State<'_, SharedService>,
    id: Uuid,
    section: SettingsSection,
    edits: Option<Vec<SettingEdit>>,
) -> Result<WorkspaceSettings> {
    let service = WorkspaceSettingsService::new(&shared, id);
    match edits {
        Some(edits) if !edits.is_empty() => service.save(section, edits).await,
        _ => service.read(section).await,
    }
}

#[tauri::command]
pub async fn workspace_readiness(shared: State<'_, SharedService>, id: Uuid) -> Result<Readiness> {
    WorkspaceSettingsService::new(&shared, id).readiness().await
}

#[tauri::command]
pub async fn workspace_policy_accounts(
    shared: State<'_, SharedService>,
    id: Uuid,
    page: u32,
    search: String,
) -> Result<PolicyAccounts> {
    WorkspaceSettingsService::new(&shared, id)
        .accounts(page, search.trim())
        .await
}

#[tauri::command]
pub async fn workspace_portal_account(
    shared: State<'_, SharedService>,
    id: Uuid,
    account: u64,
    change: Option<AccountOverride>,
) -> Result<AccountPortalSettings> {
    WorkspaceSettingsService::new(&shared, id)
        .portal_account(account, change.as_ref())
        .await
}

#[tauri::command]
pub async fn workspace_rotation_endpoints(
    shared: State<'_, SharedService>,
    id: Uuid,
) -> Result<Vec<RotationEndpoint>> {
    WorkspaceSettingsService::new(&shared, id).endpoints().await
}

#[tauri::command]
pub async fn rotate_workspace_keys(
    shared: State<'_, SharedService>,
    id: Uuid,
    plan: RotationPlan,
) -> Result<RotationReport> {
    WorkspaceSettingsService::new(&shared, id)
        .rotate(plan)
        .await
}

#[tauri::command]
pub async fn recover_workspace_rotation(
    shared: State<'_, SharedService>,
    id: Uuid,
    recovery: RotationRecovery,
) -> Result<RotationReport> {
    WorkspaceSettingsService::new(&shared, id)
        .recover(recovery)
        .await
}

#[tauri::command]
pub async fn workspace_policy_resources(
    shared: State<'_, SharedService>,
    id: Uuid,
    kind: ResourceKind,
    page: u32,
    search: String,
) -> Result<ResourcePolicies> {
    WorkspaceSettingsService::new(&shared, id)
        .resources(kind, page, search.trim())
        .await
}

#[tauri::command]
pub async fn save_workspace_resource_policy(
    shared: State<'_, SharedService>,
    id: Uuid,
    kind: ResourceKind,
    resource: u64,
    change: ResourceOverride,
) -> Result<ResourcePolicyItem> {
    WorkspaceSettingsService::new(&shared, id)
        .save_resource(kind, resource, &change)
        .await
}

#[tauri::command]
pub async fn workspace_cost_settings(
    shared: State<'_, SharedService>,
    id: Uuid,
    draft: Option<ServiceCostSettings>,
) -> Result<ServiceCostSettings> {
    WorkspaceSettingsService::new(&shared, id)
        .cost_settings(draft.as_ref())
        .await
}
