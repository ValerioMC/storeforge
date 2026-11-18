import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { errorMessage } from '../../../common/tauri'
import { bucketsApi } from '../../bucket/api/buckets'
import type { Bucket } from '../../bucket/model/bucket'
import { objectsApi } from '../api/objects'
import { demoKey, demoPrefix } from '../model/demo'
import { objectId, type Location, type ObjectId, type ObjectRow } from '../model/objects'

type BrowserTab = 'Objects' | 'Versions' | 'Settings'

interface BrowserView {
  filter: string
  technical: boolean
  recursive: boolean
  selected: string[]
  focus: ObjectId | null
  tab: BrowserTab
}

export const useBrowser = defineStore('browser', () => {
  const connection = ref('')
  const buckets = ref<Bucket[]>([])
  const bucket = ref('')
  const prefix = ref('')
  const rows = ref<ObjectRow[]>([])
  const selected = ref<string[]>([])
  const focus = ref<ObjectId | null>(null)
  const cursor = ref<string | null>(null)
  const loading = ref(false)
  const bucketLoading = ref(false)
  const error = ref('')
  const bucketError = ref('')
  const recursive = ref(false)
  const filter = ref('')
  const technical = ref(false)
  const tab = ref<BrowserTab>('Objects')
  const views = new Map<string, BrowserView>()
  let revision = ''
  let bucketRequest = 0
  let rowRequest = 0

  const location = computed<Location>(() => ({ connection: connection.value, bucket: bucket.value, prefix: prefix.value }))
  const selectedObjects = computed(() => selected.value.map(key => objectId(bucket.value, key)))

  async function loadBuckets(id = connection.value): Promise<void> {
    const request = ++bucketRequest
    bucketLoading.value = Boolean(id)
    bucketError.value = ''
    try {
      const listing = id ? await bucketsApi.list(id) : []
      if (request === bucketRequest) buckets.value = listing
    } catch (failure) {
      if (request === bucketRequest) bucketError.value = errorMessage(failure)
    } finally {
      if (request === bucketRequest) bucketLoading.value = false
    }
  }

  async function changeConnection(id: string, contextRevision = ''): Promise<void> {
    rememberView()
    bucketRequest++
    connection.value = id
    revision = contextRevision
    buckets.value = []
    bucketError.value = ''
    restoreView('', '')
    await Promise.all([loadRows(), loadBuckets(id)])
  }

  async function navigate(name: string, path = ''): Promise<void> {
    if (bucket.value === name && prefix.value === path) return
    rememberView()
    restoreView(name, path)
    await loadRows()
  }

  function viewKey(name = bucket.value, path = prefix.value): string {
    return JSON.stringify([connection.value, revision, name, path])
  }

  function rememberView(): void {
    if (!connection.value || !bucket.value) return
    const key = viewKey()
    views.delete(key)
    views.set(key, {
      filter: filter.value, technical: technical.value, recursive: recursive.value,
      selected: selected.value, focus: focus.value, tab: tab.value,
    })
    if (views.size > 80) {
      const oldest = views.keys().next().value
      if (oldest) views.delete(oldest)
    }
  }

  function restoreView(name: string, path: string): void {
    const previous = views.get(viewKey(name, path))
    bucket.value = name
    prefix.value = path
    filter.value = previous?.filter ?? ''
    technical.value = previous?.technical ?? false
    recursive.value = previous?.recursive ?? false
    selected.value = previous?.selected ?? []
    focus.value = previous ? previous.focus : connection.value === 'demo' && path === demoPrefix ? objectId(name, demoKey) : null
    tab.value = previous?.tab ?? 'Objects'
  }

  async function loadRows(append = false): Promise<void> {
    const request = ++rowRequest
    const token = append ? cursor.value : null
    const target = location.value
    if (!append) {
      rows.value = []
      cursor.value = null
    }
    error.value = ''
    loading.value = Boolean(target.bucket && target.connection)
    if (!loading.value) return
    try {
      const page = await objectsApi.list(target, token, recursive.value)
      if (request !== rowRequest) return
      rows.value = append ? [...rows.value, ...page.rows] : page.rows
      cursor.value = page.cursor
    } catch (failure) {
      if (request === rowRequest) error.value = errorMessage(failure)
    } finally {
      if (request === rowRequest) loading.value = false
    }
  }

  function toggleSelection(key: string): void {
    selected.value = selected.value.includes(key) ? selected.value.filter(value => value !== key) : [...selected.value, key]
  }

  function toggleVisible(keys: string[]): void {
    const all = keys.every(key => selected.value.includes(key))
    const changed = new Set(selected.value)
    for (const key of keys) {
      if (all) changed.delete(key)
      else changed.add(key)
    }
    selected.value = [...changed]
  }

  return {
    buckets, bucket, prefix, rows, selected, selectedObjects, focus, cursor, loading, bucketLoading, error, bucketError,
    location, recursive, filter, technical, tab, loadBuckets, changeConnection, navigate, loadRows, toggleSelection, toggleVisible,
  }
})
