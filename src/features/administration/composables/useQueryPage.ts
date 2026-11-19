import { onScopeDispose, ref, shallowRef, watch, type WatchSource } from 'vue'
import { errorMessage } from '../../../common/tauri'
import type { QueryPage } from '../model/iam'

export function useQueryPage<T>(
  scope: WatchSource<unknown>,
  load: (marker?: string) => Promise<QueryPage<T>>,
  label: (item: T) => string,
  searchPageLimit = Number.POSITIVE_INFINITY,
) {
  const items = shallowRef<T[]>([])
  const markers = ref<string[]>([])
  const nextMarker = ref<string | null>(null)
  const loading = ref(false)
  const error = ref('')
  const search = ref('')
  let generation = 0

  async function read(page: string[]): Promise<void> {
    const request = ++generation
    const query = search.value.trim().toLowerCase()
    loading.value = true
    error.value = ''
    try {
      let marker = page.at(-1)
      let result: QueryPage<T>
      let filtered: T[]
      let scanned = 0
      const visited = new Set(page)
      do {
        result = await load(marker)
        scanned++
        if (request !== generation) return
        if (result.marker && visited.has(result.marker)) throw new Error('The provider repeated a page marker.')
        if (result.marker) visited.add(result.marker)
        filtered = query ? result.items.filter(item => label(item).toLowerCase().includes(query)) : result.items
        marker = result.marker ?? undefined
      } while (query && !filtered.length && marker && scanned < searchPageLimit)
      items.value = filtered
      markers.value = page
      nextMarker.value = result.marker
    } catch (failure) {
      if (request === generation) error.value = errorMessage(failure)
    } finally {
      if (request === generation) loading.value = false
    }
  }

  function next(): void {
    if (nextMarker.value && !loading.value) void read([...markers.value, nextMarker.value])
  }

  function previous(): void {
    if (markers.value.length && !loading.value) void read(markers.value.slice(0, -1))
  }

  function reload(): Promise<void> {
    items.value = []
    nextMarker.value = null
    markers.value = []
    return read([])
  }

  watch(scope, () => void reload(), { immediate: true })
  onScopeDispose(() => { generation++ })

  return { items, markers, nextMarker, loading, error, search, next, previous, reload }
}
