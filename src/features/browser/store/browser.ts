import { computed, ref, watch } from 'vue'
import { defineStore } from 'pinia'
import { errorMessage } from '../../../common/tauri'
import { bucketsApi } from '../../bucket/api/buckets'
import type { Bucket } from '../../bucket/model/bucket'
import { countPrefix, searchListing } from '../api/listing'
import { objectsApi } from '../api/objects'
import { demoKey, demoPrefix } from '../model/demo'
import { browserLocationKey, selectedRange, type BrowserProfile } from '../model/listing'
import { objectId, type Location, type ObjectDetails, type ObjectId, type ObjectRow } from '../model/objects'
import { defaultPreferences, readFavorites, readListingPreferences, writeFavorite, writeListingPreferences, type ListingPreferences } from '../model/preferences'

type BrowserTab = 'Objects' | 'Versions' | 'Settings'

interface BrowserView {
  filter: string
  selected: string[]
  focus: ObjectId | null
  focusedRow: string | null
  tab: BrowserTab
  preferences: ListingPreferences
}

type RowDetails = { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; value: ObjectDetails }
type PrefixCount = { status: 'idle' | 'loading' } | { status: 'error'; message: string } | { status: 'ready'; objects: number; bytes: number }

function browserStore(profile: BrowserProfile) {
  const connection = ref('')
  const buckets = ref<Bucket[]>([])
  const bucket = ref('')
  const prefix = ref('')
  const rows = ref<ObjectRow[]>([])
  const selected = ref<string[]>([])
  const focus = ref<ObjectId | null>(null)
  const focusedRow = ref<string | null>(null)
  let selectionAnchor: string | null = null
  const cursor = ref<string | null>(null)
  const loading = ref(false)
  const bucketLoading = ref(false)
  const error = ref('')
  const bucketError = ref('')
  const defaults = defaultPreferences()
  const recursive = ref(defaults.recursive)
  const filter = ref('')
  const technical = ref(defaults.technical)
  const advanced = ref(defaults.advanced)
  const options = ref(defaults.options)
  const sort = ref(defaults.sort)
  const detailColumns = ref(defaults.details)
  const columnWidths = ref(defaults.columnWidths)
  const limitReached = ref(false)
  const deletedError = ref('')
  const rowDetails = ref(new Map<string, RowDetails>())
  const prefixCount = ref<PrefixCount>({ status: 'idle' })
  const recent = ref<Location[]>([])
  const favorites = ref(readFavorites())
  let restoring = false
  let detailsRevision = 0
  let countRequest = 0
  const tab = ref<BrowserTab>('Objects')
  const views = new Map<string, BrowserView>()
  let revision = ''
  let bucketRequest = 0
  let rowRequest = 0

  const location = computed<Location>(() => ({ connection: connection.value, bucket: bucket.value, prefix: prefix.value }))
  const selectedObjects = computed(() => selected.value.map(key => objectId(bucket.value, key)))
  const isFavorite = computed(() => favorites.value.some(item => browserLocationKey(item) === browserLocationKey(location.value)))
  const scopedListing = computed(() => !!filter.value || options.value.scope !== 'path' ||
    options.value.kind !== 'all' || !!options.value.storageClass || options.value.includeDeleted ||
    sort.value.column !== 'provider' || options.value.limit !== defaults.options.limit)

  watch([recursive, technical, advanced, options, sort, detailColumns, columnWidths], persistPreferences, { deep: true, flush: 'sync' })

  function persistPreferences(): void {
    if (restoring || !connection.value || !bucket.value || connection.value === 'demo') return
    writeListingPreferences(profile, viewKey(), currentPreferences())
  }

  function currentPreferences(): ListingPreferences {
    return {
      recursive: recursive.value, technical: technical.value, advanced: advanced.value, options: { ...options.value },
      sort: { ...sort.value }, details: [...detailColumns.value], columnWidths: { ...columnWidths.value },
    }
  }

  function toggleFavorite(target = location.value): void {
    if (target.bucket && target.connection) favorites.value = writeFavorite(target)
  }

  function recordRecent(): void {
    if (!location.value.bucket || !location.value.connection) return
    const key = browserLocationKey(location.value)
    recent.value = [location.value, ...recent.value.filter(entry => browserLocationKey(entry) !== key)].slice(0, 20)
  }

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
    await Promise.all([loadRows(), profile === 'root' ? loadBuckets(id) : undefined])
  }

  async function navigate(name: string, path = ''): Promise<void> {
    if (bucket.value === name && prefix.value === path) return
    rememberView()
    restoreView(name, path)
    recordRecent()
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
      filter: filter.value, selected: selected.value, focus: focus.value, focusedRow: focusedRow.value, tab: tab.value, preferences: currentPreferences(),
    })
    if (views.size > 80) {
      const oldest = views.keys().next().value
      if (oldest) views.delete(oldest)
    }
  }

  function restoreView(name: string, path: string): void {
    restoring = true
    const key = viewKey(name, path)
    const previous = views.get(key)
    const preferences = previous?.preferences ?? readListingPreferences(profile, key)
    rows.value = []
    bucket.value = name
    prefix.value = path
    filter.value = previous?.filter ?? ''
    technical.value = preferences.technical
    recursive.value = preferences.recursive
    advanced.value = preferences.advanced
    options.value = preferences.options
    sort.value = preferences.sort
    columnWidths.value = preferences.columnWidths
    detailColumns.value = preferences.details
    selected.value = previous?.selected ?? []
    focus.value = previous ? previous.focus : connection.value === 'demo' && path === demoPrefix ? objectId(name, demoKey) : null
    focusedRow.value = previous?.focusedRow ?? null
    tab.value = previous?.tab ?? 'Objects'
    selectionAnchor = null
    prefixCount.value = { status: 'idle' }
    countRequest++
    invalidateRowDetails()
    restoring = false
  }

  async function loadRows(append = false): Promise<void> {
    if (append && (loading.value || !cursor.value)) return
    const request = ++rowRequest
    const token = append ? cursor.value : null
    const target = location.value
    if (!append) {
      cursor.value = null
      limitReached.value = false
      deletedError.value = ''
    }
    error.value = ''
    loading.value = Boolean(target.bucket && target.connection)
    if (!loading.value) {
      rows.value = []
      return
    }
    try {
      if (scopedListing.value && !append) {
        const result = await searchListing({
          target, phrase: filter.value, recursive: recursive.value, options: { ...options.value }, sort: { ...sort.value },
        }, () => request === rowRequest)
        if (request !== rowRequest) return
        rows.value = result.rows
        limitReached.value = result.limitReached
        deletedError.value = result.deletedError
        return
      }
      const page = await objectsApi.list(target, token, recursive.value)
      if (request !== rowRequest) return
      rows.value = append ? [...rows.value, ...page.rows] : page.rows
      cursor.value = page.cursor
    } catch (failure) {
      if (request === rowRequest) {
        if (!append) rows.value = []
        error.value = errorMessage(failure)
      }
    } finally {
      if (request === rowRequest) loading.value = false
    }
  }

  function toggleSelection(key: string, orderedKeys: string[] = [], range = false): void {
    if (range) {
      selected.value = [...new Set([...selected.value, ...selectedRange(orderedKeys, selectionAnchor, key)])]
    } else {
      selected.value = selected.value.includes(key) ? selected.value.filter(value => value !== key) : [...selected.value, key]
    }
    selectionAnchor = key
  }

  function invalidateRowDetails(): void {
    detailsRevision++
    rowDetails.value.clear()
  }

  async function loadRowDetails(key: string): Promise<void> {
    if (rowDetails.value.has(key)) return
    const target = location.value
    const requestedRevision = detailsRevision
    rowDetails.value.set(key, { status: 'loading' })
    try {
      const value = await objectsApi.details(target.connection, objectId(target.bucket, key))
      if (requestedRevision === detailsRevision) rowDetails.value.set(key, { status: 'ready', value })
    } catch (failure) {
      if (requestedRevision === detailsRevision) rowDetails.value.set(key, { status: 'error', message: errorMessage(failure) })
    }
  }

  async function loadPrefixCount(): Promise<void> {
    const request = ++countRequest
    prefixCount.value = { status: 'loading' }
    try {
      const count = await countPrefix(location.value, () => request === countRequest)
      if (request === countRequest) prefixCount.value = { status: 'ready', ...count }
    } catch (failure) {
      if (request === countRequest) prefixCount.value = { status: 'error', message: errorMessage(failure) }
    }
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
    buckets, bucket, prefix, rows, selected, selectedObjects, focus, focusedRow, cursor, loading, bucketLoading, error, bucketError,
    location, recursive, filter, technical, tab, advanced, options, sort, detailColumns, columnWidths,
    rowDetails, prefixCount, deletedError, limitReached, recent, favorites, isFavorite,
    loadBuckets, changeConnection, navigate, loadRows, toggleSelection, toggleVisible,
    invalidateRowDetails, loadRowDetails, loadPrefixCount, toggleFavorite,
  }
}

export const useBrowser = defineStore('browser', () => browserStore('root'))
export const useEmbeddedBrowser = defineStore('embedded-browser', () => browserStore('embedded'))

export type BrowserStore = ReturnType<typeof useBrowser> | ReturnType<typeof useEmbeddedBrowser>
