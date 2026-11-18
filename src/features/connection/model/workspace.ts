import { demoBuckets, demoPrefix } from '../../browser/model/demo'
import { executionContext, identityAvailable, type Connection } from './connection'

export type DesktopWorkspace = 'browser' | 'manager' | 'cephAdmin' | 'admin'
export type ManagerPage = 'overview' | 'buckets' | 'access' | 'tools'
export type AdminPage = 'connections' | 'preferences' | 'activity'
export type WorkspaceDestination =
  | { workspace: 'browser'; bucket?: string; prefix?: string }
  | { workspace: 'manager'; page: ManagerPage; bucket?: string }
  | { workspace: 'cephAdmin' }
  | { workspace: 'admin'; page: AdminPage; jobId?: string }
export type WorkspaceRoute = WorkspaceDestination & { connectionId: string }

export const workspaceNames: Record<DesktopWorkspace, string> = {
  browser: 'Browser', manager: 'Manager', cephAdmin: 'Ceph Admin', admin: 'Admin',
}
export const managerPages: Record<ManagerPage, string> = {
  overview: 'Overview', buckets: 'Buckets', access: 'Access & topics', tools: 'Storage tools',
}
export const adminPages: Record<AdminPage, string> = {
  connections: 'Connections', preferences: 'Preferences', activity: 'Activity',
}

export function workspaceAvailable(profile: Connection | undefined, workspace: DesktopWorkspace): boolean {
  if (workspace === 'admin') return true
  if (!profile) return false
  if (profile.id === 'demo') return true
  if (workspace === 'cephAdmin') {
    const identities = ['cephAdmin', 'adminOps', 'supervision'] as const
    return identities.some(identity => identityAvailable(profile, identity))
  }
  if (!executionContext(profile, workspace)) return false
  return workspace === 'browser' ? profile.settings?.browserEnabled !== false : profile.settings?.managerEnabled !== false
}

export function defaultRoute(profile?: Connection): WorkspaceRoute {
  const connectionId = profile?.id ?? ''
  if (connectionId === 'demo') return { workspace: 'browser', connectionId, bucket: demoBuckets[0]?.name, prefix: demoPrefix }
  const preferred = profile?.settings?.defaultWorkspace
  if (preferred === 'manager' && workspaceAvailable(profile, 'manager')) return { workspace: 'manager', connectionId, page: 'overview' }
  if (preferred === 'cephAdmin' && workspaceAvailable(profile, 'cephAdmin')) return { workspace: 'cephAdmin', connectionId }
  if (preferred === 'admin') return { workspace: 'admin', connectionId, page: 'connections' }
  if (workspaceAvailable(profile, 'browser')) return { workspace: 'browser', connectionId }
  if (workspaceAvailable(profile, 'manager')) return { workspace: 'manager', connectionId, page: 'overview' }
  if (workspaceAvailable(profile, 'cephAdmin')) return { workspace: 'cephAdmin', connectionId }
  return { workspace: 'admin', connectionId, page: 'connections' }
}

export function routeTitle(route: WorkspaceRoute): string {
  if (route.workspace === 'admin') return adminPages[route.page]
  if (route.workspace === 'manager') return managerPages[route.page]
  return workspaceNames[route.workspace]
}

export function isWorkspaceRoute(value: unknown): value is WorkspaceRoute {
  if (!value || typeof value !== 'object' || !('connectionId' in value) || typeof value.connectionId !== 'string' ||
    !('workspace' in value)) return false
  if (value.workspace === 'cephAdmin') return true
  if (value.workspace === 'browser') return hasOptionalString(value, 'bucket') && hasOptionalString(value, 'prefix')
  if (!('page' in value) || typeof value.page !== 'string') return false
  if (value.workspace === 'admin') return Object.hasOwn(adminPages, value.page) && hasOptionalString(value, 'jobId')
  return value.workspace === 'manager' && Object.hasOwn(managerPages, value.page) &&
    hasOptionalString(value, 'bucket')
}

function hasOptionalString(value: object, key: 'bucket' | 'prefix' | 'jobId'): boolean {
  return !(key in value) || typeof (value as Partial<Record<typeof key, unknown>>)[key] === 'string'
}
