import { demoBuckets, demoPrefix } from '../../browser/model/demo'
import { executionContext, identityAvailable, type Connection } from './connection'

export type DesktopWorkspace = 'browser' | 'manager' | 'cephAdmin' | 'storageOps' | 'admin'
export type ManagerPage = 'overview' | 'buckets' | 'featureRules' | 'access' | 'tools' | 'metrics'
export type CephPage = 'overview' | 'buckets' | 'resources' | 'metrics' | 'health' | 'multisite'
export type AdminPage = 'overview' | 'connections' | 'preferences' | 'activity' | 'metrics' | 'health' | 'rgwIdentities'
export type MetricPage = 'usage' | 'history' | 'traffic' | 'cost'
export type CephGroup = 'Ceph accounts' | 'Ceph users' | 'Ceph buckets'
export type WorkspaceDestination =
  | { workspace: 'browser'; bucket?: string; prefix?: string }
  | { workspace: 'manager'; page: ManagerPage; bucket?: string; metric?: MetricPage; notificationTopic?: string }
  | { workspace: 'cephAdmin'; page?: CephPage; metric?: MetricPage; group?: CephGroup; bucket?: string; workbenchBucket?: string }
  | { workspace: 'storageOps'; page: 'overview' | 'buckets'; workbenchBucket?: string }
  | { workspace: 'admin'; page: AdminPage; jobId?: string; audit?: boolean; metric?: MetricPage }
export type WorkspaceRoute = WorkspaceDestination & { connectionId: string }

export const workspaceNames: Record<DesktopWorkspace, string> = {
  browser: 'Browser', manager: 'Manager', cephAdmin: 'Ceph Admin', storageOps: 'Storage Ops', admin: 'Admin',
}
export const managerPages: Record<ManagerPage, string> = {
  overview: 'Overview', metrics: 'Usage & metrics', buckets: 'Buckets', featureRules: 'Feature rules', access: 'Access & topics', tools: 'Storage tools',
}
const cephPages: Record<CephPage, string> = {
  overview: 'Overview', buckets: 'RGW buckets', resources: 'RGW resources', metrics: 'Usage & metrics', health: 'Endpoint health', multisite: 'Multisite',
}
export const adminPages: Record<AdminPage, string> = {
  overview: 'Overview', metrics: 'Usage & metrics', health: 'Endpoint health', connections: 'Connections', preferences: 'Preferences', activity: 'Activity',
  rgwIdentities: 'Managed RGW identities',
}

export function workspaceAvailable(profile: Connection | undefined, workspace: DesktopWorkspace): boolean {
  if (workspace === 'admin' || workspace === 'storageOps') return true
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
  if (preferred === 'storageOps') return { workspace: 'storageOps', connectionId, page: 'overview' }
  if (preferred === 'admin') return { workspace: 'admin', connectionId, page: 'overview' }
  if (workspaceAvailable(profile, 'browser')) return { workspace: 'browser', connectionId }
  if (workspaceAvailable(profile, 'manager')) return { workspace: 'manager', connectionId, page: 'overview' }
  if (workspaceAvailable(profile, 'cephAdmin')) return { workspace: 'cephAdmin', connectionId }
  return { workspace: 'admin', connectionId, page: 'connections' }
}

export function routeTitle(route: WorkspaceRoute): string {
  if (route.workspace === 'admin') return adminPages[route.page]
  if (route.workspace === 'manager') return managerPages[route.page]
  if (route.workspace === 'cephAdmin') return cephPages[route.page ?? 'overview']
  if (route.workspace === 'storageOps' && route.page === 'buckets') return 'Bucket inventory'
  return workspaceNames[route.workspace]
}

export function isWorkspaceRoute(value: unknown): value is WorkspaceRoute {
  if (!value || typeof value !== 'object' || !('connectionId' in value) || typeof value.connectionId !== 'string' ||
    !('workspace' in value)) return false
  if ('metric' in value && value.metric !== 'usage' && value.metric !== 'history' && value.metric !== 'traffic' && value.metric !== 'cost') return false
  if ('workbenchBucket' in value && (typeof value.workbenchBucket !== 'string' || (!('page' in value) || value.page !== 'buckets'))) return false
  if (value.workspace === 'storageOps') return 'page' in value && (value.page === 'overview' || value.page === 'buckets')
  if (value.workspace === 'cephAdmin') {
    return (!('page' in value) || typeof value.page === 'string' && Object.hasOwn(cephPages, value.page)) &&
      (!('group' in value) || value.group === 'Ceph accounts' || value.group === 'Ceph users' || value.group === 'Ceph buckets') && hasOptionalString(value, 'bucket')
  }
  if (value.workspace === 'browser') return hasOptionalString(value, 'bucket') && hasOptionalString(value, 'prefix')
  if (!('page' in value) || typeof value.page !== 'string') return false
  if (value.workspace === 'admin') return Object.hasOwn(adminPages, value.page) && hasOptionalString(value, 'jobId') && (!('audit' in value) || typeof value.audit === 'boolean')
  return value.workspace === 'manager' && Object.hasOwn(managerPages, value.page) &&
    hasOptionalString(value, 'bucket') && (!('notificationTopic' in value) ||
      value.page === 'buckets' && 'bucket' in value && typeof value.notificationTopic === 'string')
}

function hasOptionalString(value: object, key: 'bucket' | 'prefix' | 'jobId'): boolean {
  return !(key in value) || typeof (value as Partial<Record<typeof key, unknown>>)[key] === 'string'
}
