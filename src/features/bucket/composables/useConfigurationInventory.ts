import { ref, shallowReactive, watch, type Ref } from 'vue'
import { bucketsApi } from '../api/buckets'
import type { Configuration } from '../model/bucket'
import { configurations } from '../model/configuration'
import { scanConfigurations, type ConfigurationReading } from '../model/workspace'

export function useConfigurationInventory(
  connection: Readonly<Ref<string>>, buckets: Readonly<Ref<string[]>>, columns: Readonly<Ref<readonly Configuration[]>>,
) {
  const rows = shallowReactive(new Map<string, Partial<Record<Configuration, ConfigurationReading>>>())
  const pending = ref(false)
  const revision = ref(0)

  watch([connection, buckets, columns, revision], async ([id, names, features], previous, onCleanup) => {
    if (id !== previous?.[0]) rows.clear()
    let active = true
    onCleanup(() => { active = false })
    pending.value = !!(id && names.length && features.length)
    const missing = names.filter(name => features.some(feature => !rows.get(name)?.[feature]))
    const read = (bucket: string, section: Configuration): Promise<string> => {
      if (id === 'demo') return Promise.resolve(configurations.find(option => option.id === section)!.example)
      return bucketsApi.configure(id, bucket, section, { action: 'read' })
    }
    if (id) await scanConfigurations(missing, features, (bucket, feature) => {
      const cached = rows.get(bucket)?.[feature]
      if (cached && cached.status !== 'error') return Promise.resolve(cached.document)
      return read(bucket, feature)
    }, (bucket, feature, reading) => {
      rows.set(bucket, { ...rows.get(bucket), [feature]: reading })
    }, () => active)
    if (active) pending.value = false
  }, { immediate: true })

  function refresh(): void {
    rows.clear()
    revision.value++
  }

  function invalidate(bucket: string): void {
    if (rows.delete(bucket)) revision.value++
  }

  return { rows, pending, refresh, invalidate }
}
