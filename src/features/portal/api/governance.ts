import { desktop } from '../../../common/tauri'
import type {
  AccessLogQuery, AdminRequestQuery, AdminResponse, LogExport, PortalComposition, PortalCost, PortalRequest,
  PortalTraffic, ProjectSettings, ProviderAccessLogPage, QuotaDirection, QuotaUnit, SettingChange, TrafficWindow,
} from '../model/governance'
import type { Page, PortalActivity, PortalScope } from '../model/spaces'

type Read =
  | { kind: 'settings' | 'composition' }
  | { kind: 'history'; space_id?: string }
  | { kind: 'accessLogs'; query: AccessLogQuery }
  | { kind: 'traffic'; window: TrafficWindow }
  | { kind: 'cost'; month: string }

type Change =
  | { kind: 'saveSetting'; setting: SettingChange }
  | { kind: 'requestSetting'; setting: SettingChange; reason: string | null }
  | { kind: 'requestQuota'; direction: QuotaDirection; size: number; unit: QuotaUnit; reason: string | null }

function read<T>(scope: PortalScope, view: Read): Promise<T> {
  return desktop<T>('read_portal', { ...scope, operation: { kind: 'governance', view } })
}

function change<T>(scope: PortalScope, action: Change): Promise<T> {
  return desktop<T>('change_portal', { ...scope, operation: { kind: 'governance', action } })
}

export const governanceApi = {
  settings: (scope: PortalScope) => read<ProjectSettings>(scope, { kind: 'settings' }),
  history: (scope: PortalScope, space_id?: string) => read<PortalActivity[]>(scope, { kind: 'history', space_id }),
  logs: (scope: PortalScope, query: AccessLogQuery) => read<ProviderAccessLogPage>(scope, { kind: 'accessLogs', query }),
  composition: (scope: PortalScope) => read<PortalComposition>(scope, { kind: 'composition' }),
  traffic: (scope: PortalScope, window: TrafficWindow) => read<PortalTraffic>(scope, { kind: 'traffic', window }),
  cost: (scope: PortalScope, month: string) => read<PortalCost>(scope, { kind: 'cost', month }),
  saveSetting: (scope: PortalScope, setting: SettingChange) => change<ProjectSettings>(scope, { kind: 'saveSetting', setting }),
  requestSetting: (scope: PortalScope, setting: SettingChange, reason: string | null) =>
    change<PortalRequest>(scope, { kind: 'requestSetting', setting, reason }),
  requestQuota: (scope: PortalScope, direction: QuotaDirection, size: number, unit: QuotaUnit, reason: string | null) =>
    change<PortalRequest>(scope, { kind: 'requestQuota', direction, size, unit, reason }),
  requests: (scope: PortalScope) => desktop<PortalRequest[]>('read_portal', { ...scope, operation: { kind: 'sharing', view: { kind: 'requests' } } }),
  adminRequests: (id: string, query: AdminRequestQuery) => desktop<Page<PortalRequest>>('read_portal_admin_requests', { id, query }),
  respond: (id: string, request: number, response: AdminResponse) => desktop<PortalRequest>('respond_portal_request', { id, request, response }),
  exportLogs: (scope: PortalScope, query: LogExport, destination: string) =>
    desktop<void>('export_portal_access_logs', { ...scope, query, destination }),
}
