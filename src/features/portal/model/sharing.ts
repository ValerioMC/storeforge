import type { Candidate, GrantRole, ProjectRole, SpaceRole } from './spaces'

export type RequestStatus = 'pending' | 'processing' | 'approved' | 'rejected' | 'failed'

export interface MembershipReceipt {
  id: number
  status: RequestStatus
  error_message: string | null
}

export interface SpaceShare {
  user_id: number
  email: string
  role: GrantRole
}

export interface SpaceAccessSummary {
  mode: 'private' | 'all' | 'restricted'
  default_account_member_role: GrantRole | null
  owner: { email: string; display_name: string | null } | null
  effective_member_count: number
  explicit_shares: SpaceShare[]
  public_link_count: number
  can_manage_access: boolean
  can_create_public_links: boolean
}

export interface PublicLink {
  id: number
  object_name: string
  url: string
  status: string
  expires_at: string | null
}

export interface Collaborator {
  user_id: number
  email: string
  display_name: string | null
  portal_role: ProjectRole
  access_source: Candidate['access_source']
  can_review_access: boolean
}

export interface Collaborators {
  summary: { collaborator_count: number; external_access_key_count: number }
  collaborators: Collaborator[]
}

export interface CollaboratorReview {
  collaborator: Collaborator
  can_request_project_removal: boolean
  can_request_role_change: boolean
  space_accesses: {
    storage_space_id: string
    storage_space_name: string
    role: SpaceRole
    source: 'direct' | 'team' | 'owner' | 'project_manager'
    can_revoke: boolean
  }[]
}

export type KeyPermission = 'read_only' | 'read_write'

export interface PortalKey {
  access_key_id: string
  target_type: 'self' | 'external'
  status: string | null
  is_active: boolean
  deletable: boolean
  created_at: string | null
  expires_at: string | null
  external_email: string | null
  storage_space_name: string | null
  bucket_name: string | null
  permission: KeyPermission | null
}

export interface KeyReceipt extends PortalKey {
  secret_access_key: string | null
}

export interface PortalKeys {
  s3_endpoint: string | null
  force_path_style: boolean
  access_keys: PortalKey[]
  can_manage_access_keys: boolean
  can_create_external_access: boolean
  max_access_keys: number
}

export const grantRoles = [{ value: 'Viewer', label: 'Viewer' }, { value: 'Editor', label: 'Editor' }] as const
export const projectRoles = [{ value: 'portal_user', label: 'Portal member' }, { value: 'portal_manager', label: 'Portal Manager' }] as const
export const accessSources = {
  direct: 'Direct grant', group: 'Group membership', direct_and_group: 'Direct and group',
  team: 'Project team', owner: 'Ownership', project_manager: 'Portal Manager',
} as const

export function projectRoleName(role: ProjectRole): string {
  return role === 'portal_manager' ? 'Portal Manager' : 'Portal member'
}
