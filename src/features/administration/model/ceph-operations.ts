import type { BucketTarget, WorkbenchRow } from '../../bucket/model/workbench'
import type { Connection } from '../../connection/model/connection'
import { workspaceAvailable } from '../../connection/model/workspace'
import { rgwTargetLabel, rgwUserLabel, type RgwTarget, type RgwUserIdentity } from './rgw'

export type BucketOwner = { kind: 'account'; id: string; tenant: string } |
  { kind: 'user'; identity: RgwUserIdentity; account: string | null }

export type CephBucketChange =
  | { kind: 'delete'; purgeObjects: boolean; bypassGc: boolean }
  | { kind: 'link'; destination: BucketOwner }
  | { kind: 'unlink' }
  | { kind: 'index'; repair: boolean; checkObjects: boolean }
  | { kind: 'browse' }

export type CephAction =
  | { kind: 'bucket'; target: BucketTarget; owner: string | null; change: CephBucketChange }
  | { kind: 'deleteUser'; identity: RgwUserIdentity; account: string | null; purgeData: boolean }
  | { kind: 'deleteAccount'; id: string; tenant: string }

export type CephActionRequest = CephAction & { confirmation: string }

export interface CephOutcome {
  success: boolean
  status: number | null
  code: string | null
  message: string
  details: Record<string, unknown> | null
}

export interface IndexOutcome {
  target: BucketTarget
  result: CephOutcome
}

export function cephBucketLabel(target: Pick<BucketTarget, 'name' | 'tenant'>): string {
  return target.tenant ? `${target.tenant}/${target.name}` : target.name
}

export function cephConfirmation(action: CephAction): string {
  if (action.kind === 'deleteUser') return `${action.purgeData ? 'PURGE' : 'DELETE'} USER ${rgwUserLabel(action.identity)}`
  if (action.kind === 'deleteAccount') return `DELETE ACCOUNT ${action.id}`
  const bucket = cephBucketLabel(action.target)
  const change = action.change
  switch (change.kind) {
    case 'delete': return `${change.purgeObjects ? 'PURGE' : 'DELETE'} BUCKET ${bucket}${change.bypassGc ? ' BYPASS GC' : ''}`
    case 'link': return `LINK BUCKET ${bucket} TO ${rgwTargetLabel(change.destination)}`
    case 'unlink': return `UNLINK BUCKET ${bucket}`
    case 'index': return change.repair ? `REPAIR INDEX ${bucket}${change.checkObjects ? ' WITH OBJECTS' : ''}` : ''
    case 'browse': return ''
  }
}

export function ownerLookup(kind: 'user' | 'account', id: string, tenant: string): RgwTarget {
  return kind === 'account' ? { kind, id } : { kind, identity: { uid: id, tenant } }
}

export function currentBucketTarget(target: BucketTarget, profile: Connection): boolean {
  return profile.id !== 'demo' && target.source === 'cephAdmin' && target.connection === profile.id &&
    target.revision === (profile.revision ?? null) && !!target.instance
}

export function batchIndexTargets(rows: readonly WorkbenchRow[], profile: Connection): BucketTarget[] {
  if (rows.length < 1 || rows.length > 200 || rows.some(row => !currentBucketTarget(row.target, profile))) return []
  return rows.map(row => row.target)
}

export function browserTarget(row: WorkbenchRow, profile: Connection): { connection: string; revision: string | null; bucket: string } | undefined {
  if (!currentBucketTarget(row.target, profile) || !row.configurationAvailable || !workspaceAvailable(profile, 'browser')) return
  return { connection: profile.id, revision: row.target.revision, bucket: row.name }
}
