import type { SharedPrincipal } from '../../connection/model/shared-service'

export const accessScopes = {
  platform: 'Platform', rgw_account: 'RGW account', rgw_user: 'RGW user', s3_connection: 'Shared connection',
} as const

export const grantKinds = {
  direct: 'Direct', group: 'Group', platform: 'Platform', resource: 'Resource',
} as const

export const accessRights = {
  ceph_admin: 'Ceph Admin',
  storage_ops: 'Storage Ops',
  manager_bucket_compare: 'Bucket comparison',
  manager_bucket_integrity_check: 'Bucket integrity checks',
  manager_bucket_migration: 'Bucket migrations',
  manager_feature_rules: 'Bucket feature rules',
  manager_bucket_purge: 'Bucket purge',
  private_connection_create: 'Create private connections',
  managed_private_connection_provision: 'Provision managed private connections',
  browser_advanced_features: 'Advanced Browser features',
  account_administrator: 'Account administrator',
  portal_manager: 'Portal manager',
  portal_user: 'Portal user',
  manager_browser_data_access: 'Manager and Browser data access',
  rgw_user_access: 'RGW user access',
  shared_connection_access: 'Shared connection access',
} as const

export interface GovernancePage<T> {
  items: T[]
  page: number
  total: number
  hasNext: boolean
}

export interface ListingQuery {
  page: number
  search?: string
}

export interface AccessFilters {
  search?: string
  scope?: keyof typeof accessScopes
  source?: keyof typeof grantKinds
  right?: keyof typeof accessRights
  userId?: number
  groupId?: number
  targetId?: number
}

export interface AccessSource {
  kind: keyof typeof grantKinds
  groupId: number | null
  groupName: string | null
}

export interface AccessReviewRow {
  key: string
  principal: Pick<SharedPrincipal, 'id' | 'email' | 'role'> & { fullName: string | null; isActive: boolean }
  target: { id: number | null; name: string; identifier: string | null }
  scope: keyof typeof accessScopes
  rights: { code: keyof typeof accessRights; sources: AccessSource[] }[]
}

export interface GroupDraft {
  name: string
  description: string | null
}

interface NamedResource {
  id: number
  name: string
}

export interface PlatformGroup extends GroupDraft {
  id: number
  systemKey: string | null
  userDetails: { id: number; email: string; fullName: string | null }[]
  accountDetails: NamedResource[]
  s3UserDetails: NamedResource[]
  s3ConnectionDetails: NamedResource[]
  openstackCloudDetails: NamedResource[]
}

export interface PlatformConnection {
  id: number
  name: string
  endpointUrl: string
  isActive: boolean
  executionStatus: 'ready' | 'remediation_required'
  createdByEmail: string | null
  createdByFullName: string | null
}

export interface PrivateConnection {
  id: number
  name: string
  endpointUrl: string
  isShared: boolean
  isActive: boolean
  serverManaged: boolean
}
