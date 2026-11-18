import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { errorMessage } from '../../../common/tauri'
import { bucketsApi } from '../../bucket/api/buckets'
import type { Bucket } from '../../bucket/model/bucket'
import { objectsApi } from '../api/objects'
import { demoBuckets, demoKey, demoPrefix } from '../model/demo'
import { objectId, type Location, type ObjectId, type ObjectRow } from '../model/objects'

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

  async function changeConnection(id: string): Promise<void> {
    connection.value = id
    buckets.value = []
    await navigate('', '')
    if (connection.value !== id) return
    await loadBuckets(id)
    if (connection.value !== id) return
    if (id === 'demo') {
      const demoBucket = demoBuckets[0]?.name ?? ''
      await navigate(demoBucket, demoPrefix)
      if (connection.value === id) focus.value = objectId(demoBucket, demoKey)
    }
  }

  async function navigate(name: string, path = ''): Promise<void> {
    bucket.value = name
    prefix.value = path
    selected.value = []
    focus.value = null
    await loadRows()
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
    location, recursive, loadBuckets, changeConnection, navigate, loadRows, toggleSelection, toggleVisible,
  }
})
