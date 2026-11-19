import { describe, expect, it } from 'vitest'
import { batchIndexTargets, browserTarget, cephConfirmation, type CephAction } from '../src/features/administration/model/ceph-operations'
import { makeWorkbenchRow, type InventoryBucket } from '../src/features/bucket/model/workbench'
import type { Connection } from '../src/features/connection/model/connection'

const profile: Connection = { id: 'ceph', revision: 'revision', name: 'Ceph', provider: 'ceph',
  endpoint: 'https://s3.example.com', region: 'us-east-1', pathStyle: true, knownBuckets: [] }
const bucket: InventoryBucket = { name: 'archive', tenant: '', instance: 'instance', owner: 'research', createdAt: null,
  bytes: null, objects: null, quota: null }
const row = makeWorkbenchRow(profile, 'cephAdmin', bucket)

describe('Ceph administrative actions', () => {
  it('includes the qualified bucket and destructive options in the exact confirmation', () => {
    const action: CephAction = { kind: 'bucket', target: { ...row.target, tenant: 'science' }, owner: row.owner,
      change: { kind: 'delete', purgeObjects: true, bypassGc: true } }
    expect(cephConfirmation(action)).toBe('PURGE BUCKET science/archive BYPASS GC')
    expect(cephConfirmation({ ...action, change: { kind: 'index', repair: false, checkObjects: false } })).toBe('')
    expect(cephConfirmation({ ...action, change: { kind: 'index', repair: true, checkObjects: true } })).toBe('REPAIR INDEX science/archive WITH OBJECTS')
  })

  it('binds ownership confirmation to the selected user or account', () => {
    const action: CephAction = { kind: 'bucket', target: row.target, owner: row.owner,
      change: { kind: 'link', destination: { kind: 'user', identity: { tenant: 'labs', uid: 'operator' }, account: 'account' } } }
    expect(cephConfirmation(action)).toBe('LINK BUCKET archive TO labs$operator')
    expect(cephConfirmation({ kind: 'deleteUser', identity: { tenant: 'labs', uid: 'operator' }, account: 'account', purgeData: true })).toBe('PURGE USER labs$operator')
  })

  it('passes exactly 200 bound bucket targets to the dedicated batch command', () => {
    const rows = Array.from({ length: 200 }, (_, index) => makeWorkbenchRow(profile, 'cephAdmin', { ...bucket, name: `bucket-${index}` }))
    const targets = batchIndexTargets(rows, profile)
    expect(targets).toHaveLength(200)
    expect(targets.at(-1)).toEqual(rows.at(-1)?.target)
    expect(batchIndexTargets([...rows, row], profile)).toEqual([])
    expect(batchIndexTargets([row, { ...row, target: { ...row.target, revision: 'old' } }], profile)).toEqual([])
  })

  it('opens Browser only for the current profile and tenant through the ordinary S3 binding', () => {
    expect(browserTarget(row, profile)).toEqual({ connection: 'ceph', revision: 'revision', bucket: 'archive' })
    const otherTenant = makeWorkbenchRow(profile, 'cephAdmin', { ...bucket, tenant: 'science' })
    expect(browserTarget(otherTenant, profile)).toBeUndefined()
    expect(browserTarget(row, { ...profile, revision: 'changed' })).toBeUndefined()
  })
})
