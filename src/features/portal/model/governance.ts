import { portalFeatures, type AccountPolicy } from '../../administration/model/workspace-settings'
import type { MembershipReceipt, RequestStatus } from './sharing'

export type RequestType = 'portal_user_access' | 'portal_user_removal' | 'account_quota_change' | 'portal_setting_change'
export type QuotaDirection = 'increase' | 'decrease'
export type QuotaUnit = 'MiB' | 'GiB' | 'TiB'
export type TrafficWindow = 'hour' | 'day' | 'week' | 'month'

export interface PortalRequest extends MembershipReceipt {
  account_id: number
  account_name: string | null
  request_type: RequestType
  payload: { reason?: string; target_name?: string; target_email?: string; portal_role?: string; direction?: QuotaDirection;
    target_quota_value?: number; target_quota_unit?: QuotaUnit; setting?: SettingKey; mode?: string; value?: SettingValue }
  requester_email: string
  decided_by_email: string | null
  decided_at: string | null
  created_at: string
  updated_at: string
  messages: { id: number; author_email: string; message: string; created_at: string }[]
}

export interface AdminRequestQuery {
  page: number
  page_size: number
  search: string
  status?: RequestStatus
  request_type?: RequestType
  account_id?: number
}

export type AdminResponse =
  | { kind: 'approve' | 'reject'; message: string | null }
  | { kind: 'message'; message: string }

export type BooleanSettingKey = AccountPolicy | 'bucket_defaults.versioning' | 'bucket_defaults.enable_cors' | 'bucket_defaults.enable_lifecycle'
export type SettingChange =
  | { key: BooleanSettingKey; value: boolean | null }
  | { key: 'bucket_defaults.noncurrent_version_expiration_days'; value: number | null }
  | { key: 'bucket_defaults.cors_allowed_origins'; value: string[] | null }

export type SettingKey = SettingChange['key']
export type SettingValue = SettingChange['value']
export const projectSettingNames: Record<SettingKey, string> = {
  ...portalFeatures,
  'bucket_defaults.versioning': 'Versioning for new spaces',
  'bucket_defaults.enable_lifecycle': 'Lifecycle for new spaces',
  'bucket_defaults.enable_cors': 'CORS for new spaces',
  'bucket_defaults.noncurrent_version_expiration_days': 'Default history retention',
  'bucket_defaults.cors_allowed_origins': 'Allowed CORS origins',
}

type ProjectPolicy = Partial<Record<AccountPolicy, boolean | null>> & {
  bucket_defaults?: {
    versioning?: boolean | null
    enable_cors?: boolean | null
    enable_lifecycle?: boolean | null
    noncurrent_version_expiration_days?: number | null
    cors_allowed_origins?: string[] | null
  } | null
  server_access_log_retention_days?: number
  max_portal_user_access_keys?: number
}

export interface ProjectSettings {
  effective: ProjectPolicy
  project_override: ProjectPolicy
  feature_states: Partial<Record<AccountPolicy, { enabled: boolean; source: string; blocked_reason: string | null }>>
  delegated_to_portal_managers: boolean
  portal_collaborator_role_management_delegated: boolean
  portal_collaborator_addition_delegated: boolean
  can_update: boolean
}

export function settingValue(policy: ProjectPolicy, key: SettingKey): SettingValue | undefined {
  if (isBucketSetting(key)) return policy.bucket_defaults?.[key.slice('bucket_defaults.'.length) as keyof NonNullable<ProjectPolicy['bucket_defaults']>]
  return policy[key]
}

function isBucketSetting(key: SettingKey): key is Extract<SettingKey, `bucket_defaults.${string}`> {
  return key.startsWith('bucket_defaults.')
}

export const requestLabels: Record<RequestType, string> = {
  portal_user_access: 'Project access',
  portal_user_removal: 'Project removal',
  account_quota_change: 'Storage quota',
  portal_setting_change: 'Project setting',
}

export const statusOptions = [
  { value: '', label: 'All statuses' }, { value: 'pending', label: 'Pending' },
  { value: 'processing', label: 'Processing' }, { value: 'approved', label: 'Approved' },
  { value: 'rejected', label: 'Rejected' }, { value: 'failed', label: 'Failed' },
] as const

export function quotaBytes(size: number, unit: QuotaUnit): number {
  const powers = { MiB: 2, GiB: 3, TiB: 4 }
  return size * 1024 ** powers[unit]
}

export type AccessLogField = 'action' | 'space' | 'path' | 'identity' | 'result'
interface LogScope { space_id?: string; timezone_offset_minutes: number }

export interface AccessLogQuery extends LogScope {
  date: string
  offset: number
  filter?: { field: AccessLogField; value: string }
}

export interface LogExport extends LogScope {
  date_from: string
  date_to: string
}

export interface ProviderAccessLog {
  id: string
  timestamp: string
  storage_space_name: string | null
  bucket_name: string
  operation: string
  object_key: string | null
  status_code: number | null
  error_code: string | null
  requester: string | null
  requester_identity: { label: string; kind: string } | null
  client_ip: string | null
  bytes_sent: number | null
}

export interface ProviderAccessLogPage {
  entries: ProviderAccessLog[]
  total: number
  limit: number
  offset: number
}

export interface CompositionEntry {
  key: string
  label: string
  bytes: number
  count: number
}

export interface PortalComposition {
  aggregate: {
    bucket_count: number
    buckets_with_snapshot: number
    total_bytes: number
    data_type_distribution: CompositionEntry[]
    storage_class_distribution: CompositionEntry[]
    size_distribution: CompositionEntry[]
    age_distribution: CompositionEntry[]
    current_vs_noncurrent: CompositionEntry[]
  }
}

export interface PortalTraffic {
  start: string
  end: string
  totals: { bytes_in: number; bytes_out: number; ops: number; success_ops: number }
  series: { timestamp: string; bytes_in: number; bytes_out: number; ops: number }[]
}

export interface PortalCost {
  month: string
  coverage: { days_collected: number; days_in_month: number }
  cost: { currency: string | null; total_cost: number | null; storage_cost: number | null;
    egress_cost: number | null; ingress_cost: number | null; requests_cost: number | null } | null
}

export function localDate(date = new Date()): string {
  return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 10)
}
