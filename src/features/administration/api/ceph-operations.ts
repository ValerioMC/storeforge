import { Channel } from '@tauri-apps/api/core'
import { desktop } from '../../../common/tauri'
import type { BucketTarget } from '../../bucket/model/workbench'
import type { Connection } from '../../connection/model/connection'
import type { BucketOwner, CephActionRequest, CephOutcome, IndexOutcome } from '../model/ceph-operations'
import type { RgwTarget } from '../model/rgw'

export function cephAction(profile: Connection, action: CephActionRequest): Promise<CephOutcome> {
  return desktop('ceph_action', { connection: profile.id, revision: profile.revision ?? null, action })
}

export function resolveCephOwner(profile: Connection, target: RgwTarget): Promise<BucketOwner> {
  return desktop('ceph_link_owner', { connection: profile.id, revision: profile.revision ?? null, target })
}

export function checkCephIndexes(connection: string, targets: BucketTarget[], receive: (result: IndexOutcome) => void): Promise<void> {
  return desktop('ceph_index_batch', { connection, targets, onResult: new Channel<IndexOutcome>(receive) })
}
