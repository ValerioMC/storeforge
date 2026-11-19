export interface SharedServiceDraft {
  name: string
  origin: string
}

export interface SharedServiceProfile extends SharedServiceDraft {
  id: string
}

export type SharedWorkspace = 'manager' | 'browser'
export type ServiceWorkspace = SharedWorkspace | 'admin' | 'cephAdmin' | 'storageOps' | 'portal'
export type SharedAction = 'accountProfile' | 'accountSecurity' | 'privateConnections' | 'identitySecurity' |
  'apiTokens' | 'authenticationSettings' | 'adminUsers' | 'adminGroups' | 'sharedConnections' | 'accessReview' |
  { reviewUser: { id: number; search: string } } | { reviewGroup: { id: number; search: string } }

export interface SharedPrincipal {
  id: number
  email: string
  role: 'ui_admin' | 'ui_superadmin' | 'ui_user'
  active: boolean
}

export interface SharedServiceSession {
  principal: SharedPrincipal
  access: Record<ServiceWorkspace, { available: boolean; contextCount: number }>
  expiresAt: string
}

export interface SharedServiceStatus {
  profile: SharedServiceProfile
  session: SharedServiceSession | null
}

export interface SharedContext {
  id: string
  kind: 'account' | 'connection' | 's3_user' | 'portal_account' | 'openstack_project'
  name: string
  connectionScope: 'shared' | 'private' | null
  managerRole: string | null
  portalRole: string | null
  endpointName: string
  endpointUrl: string
  capabilities: { canManageIam: boolean; stsCapable: boolean; adminApiCapable: boolean }
}

export function sharedServiceNeedsAuthorization(error: unknown): boolean {
  return typeof error === 'object' && error !== null && 'kind' in error && error.kind === 'reauthenticate'
}
