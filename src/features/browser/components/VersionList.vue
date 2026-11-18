<script setup lang="ts">
import { ref, watch } from 'vue'
import { Download, History, RotateCcw, Trash2 } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Notice from '../../../components/ui/Notice.vue'
import { objectsApi } from '../api/objects'
import { objectId, type ObjectId, type ObjectVersion, type VersionCursor } from '../model/objects'

const props = defineProps<{ connection: string; bucket: string; prefix: string; exact: boolean; demo: boolean }>()
const emit = defineEmits<{ download: [object: ObjectId]; changed: [] }>()
const versions = ref<ObjectVersion[]>([])
const cursor = ref<VersionCursor | null>(null)
const loading = ref(false)
const error = ref('')
const confirm = ref<{ version: ObjectVersion; action: 'delete' | 'restoreVersion' }>()
let request = 0

watch(() => [props.connection, props.bucket, props.prefix, props.exact], () => void load(), { immediate: true })

async function load(append = false): Promise<void> {
  const id = ++request
  loading.value = true
  error.value = ''
  if (!append) {
    versions.value = []
    cursor.value = null
  }
  try {
    const page = await objectsApi.versions(props.connection, props.bucket, props.prefix, props.exact, cursor.value)
    if (id !== request) return
    versions.value = [...versions.value, ...page.versions]
    cursor.value = page.cursor
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request) loading.value = false
  }
}

async function apply(): Promise<void> {
  const selection = confirm.value
  if (!selection) return
  confirm.value = undefined
  try {
    await objectsApi.change(props.connection, objectId(props.bucket, selection.version.key, selection.version.versionId), { kind: selection.action })
    emit('changed')
    await load()
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}
</script>

<template>
  <div class="versions">
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <p v-if="loading && !versions.length" class="loading-line" role="status">Loading versions…</p>
    <EmptyState v-else-if="!versions.length && !error" :icon="History" title="No versions found" description="Versions and delete markers appear here when the endpoint returns them." />
    <div v-for="version in versions" :key="`${version.key}:${version.versionId}`" class="version-row">
      <History :size="16" />
      <div class="version-description">
        <strong v-if="!exact">{{ version.key }}</strong>
        <code :title="version.versionId">{{ version.versionId }}</code>
        <small>{{ formatDate(version.modified, true) }} · {{ version.deleteMarker ? 'Delete marker' : formatSize(version.size) }}</small>
      </div>
      <span v-if="version.latest" class="chip">Current</span>
      <button v-if="!version.deleteMarker && !version.latest" class="icon-button subtle" aria-label="Restore version" title="Restore version" :disabled="demo" @click="confirm = { version, action: 'restoreVersion' }"><RotateCcw :size="15" /></button>
      <button v-if="!version.deleteMarker" class="icon-button subtle" aria-label="Download version" title="Download version" :disabled="demo" @click="emit('download', objectId(bucket, version.key, version.versionId))"><Download :size="15" /></button>
      <button class="icon-button subtle" :aria-label="version.deleteMarker ? 'Remove delete marker' : 'Delete version'" :title="version.deleteMarker ? 'Remove delete marker' : 'Delete version'" :disabled="demo"
        @click="confirm = { version, action: 'delete' }"><Trash2 :size="15" /></button>
    </div>
    <button v-if="cursor" class="load-more" :disabled="loading" @click="load(true)">Load more versions</button>
    <ConfirmDialog v-if="confirm" :title="confirm.action === 'delete' ? 'Delete this version?' : 'Restore this version?'"
      :description="confirm.action === 'restoreVersion' ? 'The selected version will be copied to the current object key.' : confirm.version.deleteMarker ? 'Removing this marker can make the previous version visible again.' : 'This version will be permanently removed from the bucket.'"
      :action="confirm.action === 'delete' ? 'Delete version' : 'Restore version'" :danger="confirm.action === 'delete'" @close="confirm = undefined" @confirm="apply" />
  </div>
</template>
