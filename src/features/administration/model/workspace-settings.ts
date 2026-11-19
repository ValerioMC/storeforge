export type SettingsSection = 'general' | 'manager' | 'browser' | 'portal' | 'quota_notifications'
export type FeaturePolicy = 'disabled' | 'targeted' | 'default_on'
export type SettingValue = boolean | number | FeaturePolicy

export interface SettingGroup {
  name: string | null
  values: Record<string, SettingValue>
}

export interface SettingEdit {
  group: string | null
  field: string
  before: SettingValue
  value: SettingValue
}

export interface PolicySettings {
  groups: SettingGroup[]
  locks: Record<string, { forced: boolean; source: string | null }>
}

export const portalFeatures = {
  browser_access_enabled: 'Browser access',
  cost_estimation_enabled: 'Cost estimation',
  allow_private_storage_space_create: 'Private spaces',
  allow_portal_named_bucket_create: 'Named buckets',
  allow_portal_user_access_key_create: 'Personal access keys',
  allow_portal_user_external_sharing: 'External sharing',
  server_access_logging_enabled: 'Provider access logging',
  storage_space_version_cleanup_enabled: 'Version cleanup',
} as const

export type AccountPolicy = keyof typeof portalFeatures
export type OverrideValue = 'inherit' | 'allow' | 'deny'

export interface PolicyAccounts {
  items: { id: number; name: string }[]
  has_next: boolean
}

export interface AccountPortalSettings {
  admin_override: Partial<Record<AccountPolicy, boolean | null>>
  effective: Record<AccountPolicy, boolean>
}

export interface Readiness {
  environment: string
  profile: string
  status: string
  findings: {
    code: string
    label: string
    result: 'pass' | 'fail' | 'manual'
    level: 'blocked' | 'critical' | 'warning' | 'manual' | 'ok'
    severity: string | null
    message: string
    documentation_url: string
    blocks_startup: boolean
  }[]
}

export const rotationLabels = {
  endpoint_runtime: 'Runtime',
  endpoint_supervision: 'Supervision',
  account: 'Managed account',
  s3_user: 'Managed user',
  ceph_admin: 'Ceph Admin',
  endpoint_admin: 'Admin Ops',
} as const

export type RotationKind = keyof typeof rotationLabels
export type RotationPhase = 'prepared' | 'activated'
export type RotationStatus = 'rotated' | 'failed' | 'skipped'

export interface RotationIdentity {
  kind: 'runtime' | 'supervision' | 'ceph_admin'
  mode: 'managed' | 'external'
  status: string
  rotation_phase: RotationPhase | null
}

export interface RotationEndpoint {
  id: number
  name: string
  provider: string
  is_editable: boolean
  capabilities: Record<string, boolean>
  service_identities: RotationIdentity[]
}

export interface RotationPlan {
  endpoint_ids: number[]
  key_types: RotationKind[]
  deactivate_only: boolean
}

export interface RotationRecovery {
  endpoint_id: number
  kind: RotationKind
  phase: RotationPhase
  deactivate_only: boolean
}

export interface RotationReport {
  summary: Record<'rotated' | 'failed' | 'skipped' | 'deleted_old_keys' | 'disabled_old_keys', number>
  results: {
    endpoint_id: number
    endpoint_name: string
    key_type: RotationKind
    target_id: string | null
    target_label: string | null
    status: RotationStatus
    message: string | null
    rotation_phase: RotationPhase | null
  }[]
}

export const rotationKinds = Object.keys(rotationLabels) as RotationKind[]
export const featurePolicyOptions: { value: FeaturePolicy; label: string }[] = [
  { value: 'disabled', label: 'Disabled' }, { value: 'targeted', label: 'Targeted' }, { value: 'default_on', label: 'Default on' },
]

export const overrideOptions: { value: OverrideValue; label: string }[] = [
  { value: 'inherit', label: 'Inherit' }, { value: 'allow', label: 'Allow' }, { value: 'deny', label: 'Deny' },
]

const identityKinds = { runtime: 'endpoint_runtime', supervision: 'endpoint_supervision', ceph_admin: 'ceph_admin' } as const
const acronyms: Record<string, string> = { rgw: 'RGW', s3: 'S3', sts: 'STS', zip: 'ZIP', cors: 'CORS', mb: 'MB', ceph: 'Ceph' }

export function settingLabel(key: string): string {
  const words = key.split('_').map(word => acronyms[word] ?? word)
  return words.join(' ').replace(/^./, letter => letter.toUpperCase())
}

export function overrideValue(value?: boolean | null): OverrideValue {
  return value == null ? 'inherit' : value ? 'allow' : 'deny'
}

export function rotationRequiresDelete(kind: RotationKind): boolean {
  return kind === 'endpoint_runtime' || kind === 'endpoint_supervision'
}

export function rotationAvailability(endpoint: RotationEndpoint, kind: RotationKind): string {
  if (endpoint.provider !== 'ceph') return 'Ceph endpoints only'
  if ((kind === 'ceph_admin' || kind === 'endpoint_admin') && !endpoint.is_editable) return 'Deployment managed'
  if (kind === 'endpoint_admin' || kind === 'account' || kind === 's3_user') return endpoint.capabilities.admin ? '' : 'Admin Ops unavailable'
  const identity = endpoint.service_identities.find(identity => identityKinds[identity.kind] === kind)
  if (identity?.rotation_phase) return 'Rotation pending'
  if (!identity || (rotationRequiresDelete(kind) && identity.mode !== 'managed')) return 'Managed outside rotation'
  return identity.status === 'ready' ? '' : 'Identity unavailable'
}

export function pendingRotations(endpoint: RotationEndpoint): RotationRecovery[] {
  return endpoint.service_identities.flatMap(identity => {
    const kind = identityKinds[identity.kind]
    if (!identity.rotation_phase || !kind || rotationAvailability(endpoint, kind) !== 'Rotation pending') return []
    return [{ endpoint_id: endpoint.id, kind, phase: identity.rotation_phase, deactivate_only: false }]
  })
}

export type ResourceKind = 'account' | 'user' | 'connection'

export interface ResourcePolicyItem {
  id: number
  name: string
  overrides: Record<string, boolean | null>
}

export interface ResourcePolicies {
  items: ResourcePolicyItem[]
  has_next: boolean
}

export const serviceRateLabels = {
  storage_gb_month_price: 'Storage per GB-month',
  egress_gb_price: 'Egress per GB',
  ingress_gb_price: 'Ingress per GB',
  requests_per_1000_price: 'Per 1,000 requests',
} as const

export type ServiceCostRates = Record<keyof typeof serviceRateLabels, string>
export type ServiceCostModel = ServiceCostRates & { currency: string }
export type ServiceCostOverride = ServiceCostRates & { storage_endpoint_id: number }

export interface ServiceCostSettings {
  model: ServiceCostModel | null
  endpoint_overrides: ServiceCostOverride[]
}
