import type { OpenStackProfile } from './openstack'

export type Provider = 'aws' | 'ceph' | 'compatible'
export type ServiceIdentityKind = 'adminOps' | 'runtime' | 'supervision' | 'cephAdmin'
export type IdentityKind = 's3' | ServiceIdentityKind
export type CapabilityState = 'detected' | 'unavailable' | 'denied' | 'unknown'
export type Workspace = 'browser' | 'manager' | 'cephAdmin' | 'storageOps' | 'admin' | 'portal'

export interface EndpointCoordinates {
  latitude: number
  longitude: number
}

export interface EndpointSettings {
  tenant: string
  account: string
  realm: string
  defaultEndpoint: boolean
  standardTags: string[]
  administrativeTags: string[]
  caCertificate: string
  browserEnabled: boolean
  managerEnabled: boolean
  defaultWorkspace: Workspace | null
  coordinates?: EndpointCoordinates | null
  managedIam?: ManagedIamOwner | null
  managedRgw?: ManagedRgwOwner | null
  openstack?: OpenStackProfile | null
}

export interface ManagedIamOwner {
  provisioningId: string
  administrator: string
  userId: string
  userName: string
  accessKeyId: string
}

export interface ManagedRgwOwner {
  administrator: string
  tenant: string
  uid: string
  account: string
  accessKeyId: string
}

export interface ServiceIdentity {
  principalId: string
  endpoint: string
  enabled: boolean
  managedExternally: boolean
  capability: CapabilityState
}

export interface Connection {
  id: string
  name: string
  provider: Provider
  endpoint: string
  region: string
  pathStyle: boolean
  knownBuckets: string[]
  endpointId?: string | null
  revision?: string | null
  settings?: EndpointSettings
  s3Capability?: CapabilityState
  services?: Partial<Record<ServiceIdentityKind, ServiceIdentity>>
}

export interface Credentials {
  accessKey: string
  secretKey: string
  sessionToken: string | null
}

export interface ServiceDraft {
  endpoint: string
  enabled: boolean
  credentials: Credentials | null
}

export interface ProfileFields {
  name: string
  provider: Provider
  endpoint: string
  region: string
  pathStyle: boolean
  knownBuckets: string[]
  settings: EndpointSettings
}

export interface ConnectionDraft extends ProfileFields {
  id: string | null
  credentials: Credentials | null
  validateS3: boolean
  reconcile: Partial<Record<ServiceIdentityKind, ServiceDraft>>
}

export interface ConnectionCheck {
  bucketCount: number | null
  capabilities: Record<IdentityKind, CapabilityState>
}

export interface ExecutionContext {
  connectionId: string
  endpointId: string
  principalId: string
  identity: IdentityKind
  tenant: string
  account: string
  region: string
  workspace: Workspace
}

export const providerNames: Record<Provider, string> = {
  aws: 'Amazon S3',
  ceph: 'Ceph RGW',
  compatible: 'S3 compatible',
}

export const serviceIdentities: readonly ServiceIdentityKind[] = ['adminOps', 'runtime', 'supervision', 'cephAdmin']
export const identityNames: Record<IdentityKind, string> = {
  s3: 'S3', adminOps: 'Admin Ops', runtime: 'Runtime', supervision: 'Supervision', cephAdmin: 'Ceph Admin',
}
export const capabilityNames: Record<CapabilityState, string> = {
  detected: 'Detected', unavailable: 'Unavailable', denied: 'Denied', unknown: 'Unknown',
}

export function endpointSettings(): EndpointSettings {
  return {
    tenant: '', account: '', realm: '', defaultEndpoint: false, standardTags: [], administrativeTags: [], caCertificate: '',
    browserEnabled: true, managerEnabled: true, defaultWorkspace: null, coordinates: null,
  }
}

export function identityCapability(profile: Connection | undefined, identity: IdentityKind): CapabilityState {
  if (!profile) return 'unavailable'
  if (identity === 's3') return profile.s3Capability ?? 'unknown'
  if (identity !== 'runtime' && profile.provider !== 'ceph') return 'unavailable'
  const service = profile.services?.[identity]
  return service?.enabled ? service.capability : 'unavailable'
}

export function identityAvailable(profile: Connection | undefined, identity: IdentityKind): boolean {
  const capability = identityCapability(profile, identity)
  return capability === 'detected' || (identity === 's3' && capability === 'unknown')
}

export function executionContext(profile: Connection, workspace: Workspace, identity: IdentityKind = 's3'): ExecutionContext | undefined {
  const principalId = identity === 's3' ? profile.id : profile.services?.[identity]?.principalId
  if (!principalId || !identityAvailable(profile, identity)) return undefined
  return {
    connectionId: profile.id, endpointId: profile.endpointId ?? profile.id,
    principalId, identity, tenant: profile.settings?.tenant ?? '',
    account: profile.settings?.account ?? '', region: profile.region, workspace,
  }
}

export function sameRealmEndpoints(profile: Connection | undefined, profiles: Connection[]): Connection[] {
  if (profile?.provider !== 'ceph' || !profile.settings?.realm) return []
  const realm = profile.settings.realm
  return profiles.filter(candidate => candidate.provider === 'ceph' && candidate.settings?.realm === realm &&
    (candidate.endpointId ?? candidate.id) !== (profile.endpointId ?? profile.id))
}


export function validCoordinates(latitude: unknown, longitude: unknown): boolean {
  return typeof latitude === 'number' && Number.isFinite(latitude) && Math.abs(latitude) <= 90 &&
    typeof longitude === 'number' && Number.isFinite(longitude) && Math.abs(longitude) <= 180
}
