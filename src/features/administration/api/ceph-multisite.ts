import { desktop } from '../../../common/tauri'
import type { FlowScope, JournalPage, JournalScope, Observation, ProfileScope, Replication, ReplicationScope, SyncStatus, Topology } from '../model/ceph-multisite'

export function readMultisiteTopology(scope: ProfileScope): Promise<Topology> {
  return desktop('ceph_multisite_topology', { scope })
}

export function readMultisiteFlow(scope: FlowScope): Promise<Observation<SyncStatus>> {
  return desktop('ceph_multisite_flow', { scope })
}

export function readMultisiteReplication(scope: ReplicationScope): Promise<Replication> {
  return desktop('ceph_multisite_replication', { scope })
}

export function readMultisiteJournal(scope: JournalScope): Promise<Observation<JournalPage>> {
  return desktop('ceph_multisite_journal', { scope })
}

export function exportCephJournal(document: string): Promise<boolean> {
  return desktop('export_ceph_journal', { document })
}
