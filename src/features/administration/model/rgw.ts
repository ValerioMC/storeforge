import type { Connection } from '../../connection/model/connection'

export interface RgwUserIdentity {
  tenant: string
  uid: string
}

export type RgwTarget = { kind: 'account'; id: string } | { kind: 'user'; identity: RgwUserIdentity }
export type RgwKind = RgwTarget['kind']

export interface RgwRow {
  target: RgwTarget
  managed: boolean
  root: RgwUserIdentity | null
}

export interface RgwAccountLimits {
  maxUsers: number | null
  maxBuckets: number | null
  maxRoles: number | null
  maxGroups: number | null
  maxAccessKeys: number | null
}

export interface RgwQuota {
  enabled: boolean
  maxBytes: number | null
  maxObjects: number | null
}

export interface RgwUsage {
  bytes: number | null
  objects: number | null
  buckets: number | null
}

export interface RgwAccount {
  id: string
  tenant: string
  name: string
  email: string
  limits: RgwAccountLimits
  quota: RgwQuota | null
  bucketQuota: RgwQuota | null
  usage: RgwUsage | null
  userCount: number | null
  root: RgwUserIdentity | null
}

export interface RgwUser {
  identity: RgwUserIdentity
  displayName: string
  email: string
  account: string | null
  accountRoot: boolean | null
  suspended: boolean | null
  maxBuckets: number | null
  quota: RgwQuota | null
  bucketQuota: RgwQuota | null
  usage: RgwUsage | null
  keys: RgwKey[] | null
  connectionKeyId: string | null
}

export interface RgwKey {
  id: string
  active: boolean | null
  metadata: RgwKeyMetadata
  enforcement: { state: 'pending' | 'disabled' } | { state: 'failed'; message: string } | null
  privateProfile: string | null
  issue: string | null
  providerMissing: boolean
}

export interface RgwKeyMetadata {
  name: string
  notes: string
  expiresAt: string | null
}

export interface RgwAccountDraft {
  name: string
  email: string | null
  limits: RgwAccountLimits
}

export interface RgwUserDraft {
  displayName: string | null
  email: string | null
  suspended: boolean | null
  maxBuckets: number | null
}

export type RgwKeyChange =
  | { action: 'create'; metadata: RgwKeyMetadata; private: boolean }
  | { action: 'metadata'; id: string; metadata: RgwKeyMetadata }
  | { action: 'status'; id: string; active: boolean }
  | { action: 'delete'; id: string }

export type RgwKeyScope = 'inventory' | 'delegated'
export type RgwChange =
  | { action: 'importAccounts'; entries: { id: string; root: RgwUserIdentity | null }[] }
  | { action: 'importUsers'; identities: RgwUserIdentity[] }
  | { action: 'createAccount'; id: string | null; tenant: string; draft: RgwAccountDraft; rootUid: string | null; managed: boolean }
  | { action: 'updateAccount'; id: string; draft: RgwAccountDraft }
  | { action: 'createUser'; identity: RgwUserIdentity; draft: RgwUserDraft; account: string | null; root: boolean; managed: boolean }
  | { action: 'updateUser'; identity: RgwUserIdentity; draft: RgwUserDraft }
  | { action: 'setQuota'; target: RgwTarget; bucket: boolean; quota: RgwQuota }
  | { action: 'key'; scope: RgwKeyScope; owner: RgwUserIdentity; change: RgwKeyChange }

export interface IssuedRgwKey {
  accessKey: string
  secretKey: string
}

export interface RgwChangeResult {
  target: RgwTarget | null
  issued: IssuedRgwKey | null
  profile: Connection | null
  issue: string | null
}

export function rgwUserLabel(identity: RgwUserIdentity): string {
  return identity.tenant ? `${identity.tenant}$${identity.uid}` : identity.uid
}

export function rgwTargetLabel(target: RgwTarget): string {
  return target.kind === 'account' ? target.id : rgwUserLabel(target.identity)
}

export function parseRgwUserLabel(value: string): RgwUserIdentity {
  const separator = value.indexOf('$')
  if (separator === -1) return { tenant: '', uid: value }
  return { tenant: value.slice(0, separator), uid: value.slice(separator + 1) }
}

export function rgwTargetKey(target: RgwTarget): string {
  return JSON.stringify([target.kind, rgwTargetLabel(target)])
}
