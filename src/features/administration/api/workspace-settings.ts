import { desktop } from '../../../common/tauri'
import type {
  AccountPolicy, AccountPortalSettings, PolicyAccounts, PolicySettings, Readiness,
  ResourceKind, ResourcePolicies, ResourcePolicyItem, ServiceCostSettings, RotationEndpoint, RotationPlan, RotationRecovery, RotationReport, SettingEdit, SettingsSection,
} from '../model/workspace-settings'

export const workspaceSettingsApi = {
  settings: (id: string, section: SettingsSection, edits?: SettingEdit[]) =>
    desktop<PolicySettings>('workspace_policy_settings', { id, section, edits }),
  resources: (id: string, kind: ResourceKind, page: number, search: string) =>
    desktop<ResourcePolicies>('workspace_policy_resources', { id, kind, page, search }),
  changeResource: (id: string, kind: ResourceKind, resource: number, policy: string, value: boolean | null) =>
    desktop<ResourcePolicyItem>('save_workspace_resource_policy', { id, kind, resource, change: { policy, value } }),
  costSettings: (id: string, draft?: ServiceCostSettings) =>
    desktop<ServiceCostSettings>('workspace_cost_settings', { id, draft }),
  readiness: (id: string) => desktop<Readiness>('workspace_readiness', { id }),
  accounts: (id: string, page: number, search: string) =>
    desktop<PolicyAccounts>('workspace_policy_accounts', { id, page, search }),
  portalAccount: (id: string, account: number, change?: { policy: AccountPolicy; value: boolean | null }) =>
    desktop<AccountPortalSettings>('workspace_portal_account', { id, account, change }),
  endpoints: (id: string) => desktop<RotationEndpoint[]>('workspace_rotation_endpoints', { id }),
  rotate: (id: string, plan: RotationPlan) => desktop<RotationReport>('rotate_workspace_keys', { id, plan }),
  recover: (id: string, recovery: RotationRecovery) => desktop<RotationReport>('recover_workspace_rotation', { id, recovery }),
}
