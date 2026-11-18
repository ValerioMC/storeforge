import { desktop, isTauri } from '../../../common/tauri'
import { demoBuckets } from '../../browser/model/demo'
import type { Bucket, Configuration, ConfigurationChange } from '../model/bucket'

export const bucketsApi = {
  list: (connection: string) => connection === 'demo' ? Promise.resolve(demoBuckets) : desktop<Bucket[]>('list_buckets', { connection }),
  create: (connection: string, bucket: string, objectLock: boolean) => desktop<void>('create_bucket', { connection, bucket, objectLock }),
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
