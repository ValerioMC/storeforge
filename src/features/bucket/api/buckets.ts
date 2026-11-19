import { desktop, isTauri } from '../../../common/tauri'
import { demoBuckets } from '../../browser/model/demo'
import type { Bucket, BucketCreation, BucketQuota, Configuration, ConfigurationChange, CreateBucket } from '../model/bucket'

export const bucketsApi = {
  list: (connection: string) => connection === 'demo' ? Promise.resolve(demoBuckets) : desktop<Bucket[]>('list_buckets', { connection }),
  create: (connection: string, bucket: string, options: CreateBucket) => desktop<BucketCreation>('create_bucket', { connection, bucket, ...options }),
  finishVersioning: (connection: string, bucket: string, location: string) => desktop<void>('finish_bucket_versioning', { connection, bucket, location }),
  quota: (connection: string, bucket: string) => desktop<BucketQuota>('read_bucket_quota', { connection, bucket }),
  updateQuota: (connection: string, bucket: string, expected: BucketQuota, draft: BucketQuota) =>
    desktop<void>('update_bucket_quota', { connection, bucket, update: { expected, draft } }),
  importBackup: () => desktop<string | null>('import_bucket_backup'),
  exportBackup: (bucket: string, document: string) => desktop<boolean>('export_bucket_configuration', { name: `${bucket}-configuration-backup.json`, document }),
  remove: (connection: string, bucket: string) => desktop<void>('delete_bucket', { connection, bucket }),
  exportConfiguration,
  configure: (connection: string, bucket: string, configuration: Configuration, change: ConfigurationChange) => desktop<string>('configure_bucket', { connection, bucket, configuration, change }),
}

async function exportConfiguration(bucket: string, configuration: Configuration, format: 'XML' | 'JSON', document: string): Promise<boolean> {
  const name = `${bucket}-${configuration}.${format.toLowerCase()}`
  if (isTauri()) return desktop<boolean>('export_bucket_configuration', { name, configuration, document })
  const url = URL.createObjectURL(new Blob([document], { type: `application/${format.toLowerCase()}` }))
  const link = window.document.createElement('a')
  link.href = url
  link.download = name
  link.click()
  window.setTimeout(() => URL.revokeObjectURL(url), 1000)
  return true
}
