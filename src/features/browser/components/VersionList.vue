<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue'
import { Download, Eye, History, RotateCcw, Trash2 } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Notice from '../../../components/ui/Notice.vue'
import { verifyVersionChange, type VerifiedVersion } from '../api/versions'
import { objectsApi } from '../api/objects'
import { objectId, type ObjectId, type ObjectVersion, type VersionCursor } from '../model/objects'

const props = defineProps<{ connection: string; revision?: string; bucket: string; prefix: string; exact: boolean; demo: boolean }>()
const emit = defineEmits<{ download: [object: ObjectId]; view: [version: ObjectVersion]; changed: [version: VerifiedVersion] }>()
const versions = ref<ObjectVersion[]>([])
const cursor = ref<VersionCursor | null>(null)
const loading = ref(false)
const error = ref('')
const applying = ref(false)
const result = ref('')
const confirm = ref<{ version: ObjectVersion; action: 'delete' | 'restoreVersion' }>()
let request = 0

watch(() => [props.connection, props.revision, props.bucket, props.prefix, props.exact], () => {
  applying.value = false
  confirm.value = undefined
  result.value = ''
  void load()
}, { immediate: true })
onUnmounted(() => { request++ })

async function load(append = false): Promise<void> {
  if (append && (loading.value || applying.value)) return
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
    const merged = new Map([...versions.value, ...page.versions].map(version => [JSON.stringify([version.key, version.versionId]), version]))
    versions.value = [...merged.values()]
    cursor.value = page.cursor
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request) loading.value = false
  }
}

async function apply(): Promise<void> {
  const selection = confirm.value
  if (!selection || applying.value) return
  confirm.value = undefined
  error.value = ''
  result.value = ''
  applying.value = true
  const id = request
  const connection = props.connection
  const object = objectId(props.bucket, selection.version.key, selection.version.versionId)
  const previous = versions.value.find(version => version.key === object.key && version.latest)
  try {
    const restoredVersionId = await objectsApi.change(connection, object, { kind: selection.action })
    const state = await verifyVersionChange(connection, object, selection.action, previous, restoredVersionId ?? undefined)
    if (id !== request) return
    await load()
    if (request !== id + 1) return
    result.value = versionResult(selection.version, selection.action, state)
    emit('changed', state)
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request || id + 1 === request) applying.value = false
  }
}

function versionResult(version: ObjectVersion, action: 'delete' | 'restoreVersion', state: VerifiedVersion): string {
  if (action === 'restoreVersion') return 'The restored object is current.'
  if (!version.deleteMarker) return 'The provider no longer lists the deleted version.'
  if (!state.latest) return 'The marker was removed. No version remains.'
  if (state.latest.deleteMarker) return 'The marker was removed. Another delete marker is current.'
  return 'The previous version is visible again.'
}
</script>

<template>
  <div class="versions">
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <Notice v-if="result" tone="success">{{ result }}</Notice>
    <button v-if="error" class="subtle" :disabled="loading || applying" @click="result = ''; load()">Refresh versions</button>
    <p v-if="loading && !versions.length" class="loading-line" role="status">Loading versions…</p>
    <EmptyState v-else-if="!versions.length && !error" :icon="History" title="No versions found" description="Versions and delete markers appear here when the endpoint returns them." />
    <div v-for="version in versions" :key="JSON.stringify([version.key, version.versionId])" class="version-row">
      <History :size="16" />
      <div class="version-description">
        <strong v-if="!exact">{{ version.key }}</strong>
        <code :title="version.versionId">{{ version.versionId }}</code>
        <small>{{ formatDate(version.modified, true) }} · {{ version.deleteMarker ? 'Delete marker' : formatSize(version.size) }}</small>
      </div>
      <span v-if="version.latest" class="chip">Current</span>
      <button v-if="!version.deleteMarker" class="icon-button subtle" aria-label="View version" title="View version" @click="emit('view', version)"><Eye :size="15" /></button>
      <button v-if="!version.deleteMarker && !version.latest" class="icon-button subtle" aria-label="Restore version" title="Restore version" :disabled="demo || applying" @click="confirm = { version, action: 'restoreVersion' }"><RotateCcw :size="15" /></button>
      <button v-if="!version.deleteMarker" class="icon-button subtle" aria-label="Download version" title="Download version" :disabled="demo" @click="emit('download', objectId(bucket, version.key, version.versionId))"><Download :size="15" /></button>
      <button class="icon-button subtle" :aria-label="version.deleteMarker ? (version.latest ? 'Restore deleted object' : 'Remove delete marker') : 'Delete version'" :title="version.deleteMarker ? (version.latest ? 'Restore deleted object' : 'Remove delete marker') : 'Delete version'" :disabled="demo || applying"
        @click="confirm = { version, action: 'delete' }"><Trash2 :size="15" /></button>
    </div>
    <button v-if="cursor" class="load-more" :disabled="loading || applying" @click="load(true)">Load more versions</button>
    <ConfirmDialog v-if="confirm" :title="confirm.version.deleteMarker ? 'Remove this delete marker?' : confirm.action === 'delete' ? 'Delete this version?' : 'Restore this version?'"
      :description="confirm.action === 'restoreVersion' ? 'The selected version will be copied to the current object key.' : confirm.version.deleteMarker ? 'Removing this marker can make the previous version visible again.' : 'This version will be permanently removed from the bucket.'"
      :action="confirm.version.deleteMarker ? 'Remove marker' : confirm.action === 'delete' ? 'Delete version' : 'Restore version'" :danger="confirm.action === 'delete'" @close="confirm = undefined" @confirm="apply" />
  </div>
</template>
