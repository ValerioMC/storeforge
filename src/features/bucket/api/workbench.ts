import { Channel } from '@tauri-apps/api/core'
import { desktop, isTauri } from '../../../common/tauri'
import { demoBuckets } from '../../browser/model/demo'
import type { Connection } from '../../connection/model/connection'
import { demoMonitoring } from '../../monitoring/model/demo'
import type { Configuration, ConfigurationChange } from '../model/bucket'
import { configurations } from '../model/configuration'
import type { BucketTarget, ExportFormat, InventoryBucket, InventorySource } from '../model/workbench'

export async function streamBucketInventory(
  profile: Connection, source: InventorySource, receive: (page: InventoryBucket[]) => void,
): Promise<void> {
  if (profile.id === 'demo') {
    receive(demoBuckets.map(bucket => {
      const sample = demoMonitoring.usage?.buckets.find(row => row.bucket === bucket.name)
      return {
        ...bucket, tenant: profile.settings?.tenant ?? '', instance: source === 'cephAdmin' ? `demo-${bucket.name}` : null,
        owner: source === 'cephAdmin' ? 'asteria-research' : null,
        bytes: sample?.usage.value?.currentBytes ?? null, objects: sample?.usage.value?.currentObjects ?? null, quota: null,
      }
    }))
    return
  }
  const pages = new Channel<InventoryBucket[]>(receive)
  await desktop('bucket_workbench_inventory', { connection: profile.id, revision: profile.revision ?? null, source, pages })
}

export function scopedBucketConfiguration(target: BucketTarget, configuration: Configuration, change: ConfigurationChange): Promise<string> {
  if (target.connection === 'demo' && change.action === 'read') return Promise.resolve(configurations.find(option => option.id === configuration)!.example)
  return desktop('bucket_workbench_configuration', { target, configuration, change })
}

export async function saveBucketSelection(format: ExportFormat, document: string): Promise<boolean> {
  if (isTauri()) return desktop('export_bucket_inventory', { format, document })
  const extension = format === 'text' ? 'txt' : format
  const url = URL.createObjectURL(new Blob([document], { type: format === 'json' ? 'application/json' : `text/${format === 'csv' ? 'csv' : 'plain'}` }))
  const link = window.document.createElement('a')
  link.href = url
  link.download = `bucket-selection.${extension}`
  link.click()
  window.setTimeout(() => URL.revokeObjectURL(url), 1000)
  return true
}
