export type ProjectRole = 'portal_user' | 'portal_manager'
export type SpaceRole = 'Viewer' | 'Editor' | 'Owner' | 'Manager'
export type GrantRole = 'Viewer' | 'Editor'
export type SpaceStatus = 'Active' | 'Attention' | 'Archived'
export type SpaceIcon = 'bucket' | 'folder' | 'archive' | 'database' | 'media'
export type VersioningStatus = 'Enabled' | 'Suspended' | 'Disabled'

export interface PortalProject {
  id: number
  name: string
  portal_role: ProjectRole
  storage_endpoint_name: string
  storage_endpoint_url: string
}

export interface PortalScope {
  id: string
  project: number
}

export interface SpaceScope extends PortalScope {
  space: string
}

export interface PortalState {
  can_manage_buckets: boolean
  can_create_private_storage_spaces: boolean
  can_create_team_storage_spaces: boolean
  can_create_external_sharing: boolean
  can_manage_portal_users: boolean
  allow_named_bucket_create: boolean
  server_access_logging_enabled: boolean
  storage_space_version_cleanup_enabled: boolean
  bucket_usage_stats_enabled: boolean
  cost_estimation_enabled: boolean
}

export interface SpaceUsage {
  id: string
  name: string
  used_bytes: number | null
  object_count: number | null
  quota_max_size_bytes: number | null
}

export interface PortalSpace extends SpaceUsage {
  role: SpaceRole
  status: SpaceStatus
  can_browse: boolean
  can_delete: boolean
  can_take_ownership: boolean
  description: string | null
  owner_label: string | null
  collaborator_count: number
  visibility: 'private' | 'shared'
  share_scope: 'restricted' | 'account'
  account_member_role: GrantRole | null
  name_editable: boolean
  created_at: string | null
  icon: { source: 'preset' | 'uploaded'; preset: SpaceIcon | null }
}

export interface PortalUsage {
  used_bytes: number | null
  used_objects: number | null
  quota_max_size_bytes: number | null
  quota_max_objects: number | null
  max_buckets: number | null
  storage_spaces: SpaceUsage[]
  other_storage_space: SpaceUsage | null
}

export interface PortalTrends {
  available: boolean
  unavailable_reason: string | null
  points: { period_start: string; used_bytes: number; max_usage_ratio_pct: number | null }[]
}

export interface PortalHealth {
  generated_at: string
  endpoints: { name: string; status: string; is_stale: boolean; checked_at: string | null }[]
}

export interface PortalAlert {
  id: string
  title: string
  description: string
  severity_label: string
  storage_space_id: string | null
}

export interface PortalActivity {
  id: number
  created_at: string
  actor: string
  action: string
  target: string
  storage_space_id: string | null
  status: string
}

export interface SpaceSettingsDraft {
  versioning_enabled: boolean
  lifecycle_enabled: boolean
  version_history_retention_days: number
}

export interface SpaceSettings extends SpaceSettingsDraft {
  versioning_status: VersioningStatus
  can_update: boolean
}

export interface SpaceStatistics {
  snapshot: {
    calculated_at: string
    total_bytes: number
    current_version_count: number
  } | null
}

export interface PortalFile {
  key: string
  size: number
  last_modified: string | null
}

export interface FileListing {
  prefixes: string[]
  objects: PortalFile[]
  next_continuation_token: string | null
}

export interface FileDetail {
  key: string
  name: string
  size: number | null
  last_modified: string | null
  content_type: string | null
  storage_class: string | null
  encryption: string | null
  preview_type: 'text' | 'image' | 'unavailable'
  preview_text: string | null
  preview_unavailable_reason: string | null
}

export interface FileVersion {
  key: string
  version_id: string
  is_latest: boolean
  is_delete_marker: boolean
  last_modified: string | null
  size: number | null
}

interface VersionPage {
  versioning_status: VersioningStatus
  can_restore: boolean
  is_truncated: boolean
  next_key_marker: string | null
  next_version_id_marker: string | null
}

export interface FileVersions extends VersionPage {
  versions: FileVersion[]
}

export interface TrashItem {
  key: string
  name: string
  deleted_at: string | null
  previous_version_id: string | null
  size: number | null
}

export interface TrashListing extends VersionPage {
  items: TrashItem[]
}

export interface SpaceOperationResult {
  status: 'completed' | 'partial' | 'failed' | 'canceled'
  restored_objects?: number
  failed_objects?: number
  failures?: { key: string; detail: string }[]
  deleted_versions?: number
  deleted_delete_markers?: number
}

export type SpaceAudience = 'private' | 'team' | 'selected'
export interface InitialShare { user_id: number; role: GrantRole }
export interface SpaceProvisioning {
  description: string | null
  visibility: 'private' | 'shared'
  share_scope: 'restricted' | 'account'
  account_member_role: GrantRole | null
  initial_shares: InitialShare[]
}

export interface Candidate {
  user_id: number
  email: string
  display_name: string | null
  access_source: 'direct' | 'group' | 'direct_and_group'
  already_shared: boolean
}

export interface Page<T> {
  items: T[]
  total: number
  page: number
  page_size: number
  total_pages: number
}

export function spaceAccess(space: PortalSpace) {
  const managed = space.role === 'Owner' || space.role === 'Manager'
  return {
    managed,
    browse: space.can_browse,
    write: space.can_browse && space.role !== 'Viewer' && space.status !== 'Archived',
    delete: space.can_delete,
    takeOwnership: space.can_take_ownership,
  }
}

export function provisionSpace(audience: SpaceAudience, description: string, teamRole: GrantRole, people: InitialShare[]): SpaceProvisioning {
  return {
    description: description.trim() || null,
    visibility: audience === 'private' ? 'private' : 'shared',
    share_scope: audience === 'team' ? 'account' : 'restricted',
    account_member_role: audience === 'team' ? teamRole : null,
    initial_shares: audience === 'selected' ? people : [],
  }
}

export function parentFolder(prefix: string): string {
  const value = prefix.replace(/\/$/, '')
  return value.slice(0, value.lastIndexOf('/') + 1)
}

export function fileName(key: string): string {
  return key.replace(/\/$/, '').split('/').at(-1) ?? key
}
