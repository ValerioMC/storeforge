<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { Download } from 'lucide-vue-next'
import { save } from '@tauri-apps/plugin-dialog'
import { formatDate, formatSize } from '../../../common/format'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import type { FileView } from '../model/navigation'
import { fileName, type FileDetail, type FileVersion, type FileVersions, type SpaceScope } from '../model/spaces'

const props = defineProps<{ scope: SpaceScope; objectKey: string; view: FileView; write: boolean; trash: boolean; sharing: boolean }>()
const emit = defineEmits<{ close: []; view: [view: FileView]; changed: []; share: [] }>()
const info = usePortalTask<FileDetail>()
const history = usePortalTask<FileVersions>()
const mutation = usePortalTask<boolean>()
const preview = usePortalTask<ArrayBuffer>()
const { data: detail } = info
const { data: versions } = history
const { pending, error } = mutation
const { error: previewError } = preview
const imageUrl = ref('')
const confirmation = ref<{ kind: 'delete' } | { kind: 'restore'; version: FileVersion }>()
const status = ref('')
const tabs = [{ value: 'details', label: 'Details' }, { value: 'versions', label: 'Version history' }] as const

watch(() => props.view, view => {
  if (view === 'details' && !props.trash && !detail.value) void loadDetail()
  if (view === 'versions') void loadVersions()
}, { immediate: true })
onBeforeUnmount(() => { if (imageUrl.value) URL.revokeObjectURL(imageUrl.value) })

async function loadDetail(): Promise<void> {
  const file = await info.run(() => portalApi.detail(props.scope, props.objectKey))
  if (file?.preview_type !== 'image' || typeof file.content_type !== 'string' || !/^image\/(png|jpeg|gif|webp|bmp|avif)$/.test(file.content_type)) return
  const bytes = await preview.run(() => portalApi.preview(props.scope, props.objectKey))
  if (bytes) imageUrl.value = URL.createObjectURL(new Blob([bytes], { type: file.content_type }))
}

async function loadVersions(append = false): Promise<void> {
  const current = append ? versions.value : undefined
  await history.run(async () => {
    const page = await portalApi.versions(props.scope, props.objectKey, {
      key_marker: current?.next_key_marker ?? undefined, version_id_marker: current?.next_version_id_marker ?? undefined,
    })
    return current ? { ...page, versions: [...current.versions, ...page.versions] } : page
  })
}

async function download(version?: string): Promise<void> {
  status.value = ''
  const finished = await mutation.run(async () => {
    const destination = await save({ defaultPath: fileName(props.objectKey), title: 'Download file' })
    if (!destination) return false
    await portalApi.download(props.scope, props.objectKey, version, destination)
    return true
  })
  if (finished) status.value = 'Download saved.'
}

async function apply(): Promise<void> {
  const action = confirmation.value
  if (!action || !props.write || pending.value) return
  const finished = await mutation.run(async () => {
    if (action.kind === 'delete') await portalApi.deleteFile(props.scope, props.objectKey)
    else await portalApi.restore(props.scope, props.objectKey, action.version.version_id)
    return true
  })
  if (!finished) return
  confirmation.value = undefined
  emit('changed')
  if (action.kind === 'delete') emit('close')
  else {
    status.value = 'Version restored.'
    await loadVersions()
  }
}
</script>

<template>
  <Modal :title="fileName(objectKey)" drawer @close="!pending && emit('close')">
    <div class="modal-form">
      <p class="muted portal-note">{{ objectKey }}</p>
      <WorkspaceTabs :model-value="view" label="File views" :tabs="tabs" @update:model-value="emit('view', $event)" />
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="status" tone="success">{{ status }}</Notice>
      <template v-if="view === 'details'">
        <Notice v-if="trash">This file is deleted. Open its version history to review recoverable versions.</Notice>
        <Notice v-else-if="info.error.value" tone="error">{{ info.error.value }}</Notice>
        <p v-else-if="info.pending.value" class="muted portal-note" role="status">Loading file details…</p>
        <template v-else-if="detail">
          <dl class="setup-summary">
            <div><dt>Size</dt><dd>{{ formatSize(detail.size) }}</dd></div>
            <div><dt>Modified</dt><dd>{{ formatDate(detail.last_modified, true) }}</dd></div>
            <div><dt>Content type</dt><dd>{{ detail.content_type || '—' }}</dd></div>
            <div><dt>Storage class</dt><dd>{{ detail.storage_class || '—' }}</dd></div>
            <div><dt>Encryption</dt><dd>{{ detail.encryption || '—' }}</dd></div>
          </dl>
          <Notice v-if="previewError" tone="error">{{ previewError }}</Notice>
          <img v-else-if="imageUrl" class="file-preview" :src="imageUrl" :alt="detail.name" />
          <pre v-else-if="detail.preview_type === 'text'" class="file-preview">{{ detail.preview_text }}</pre>
          <p v-else-if="detail.preview_unavailable_reason" class="muted portal-note">{{ detail.preview_unavailable_reason }}</p>
          <div v-if="!confirmation" class="portal-inline">
            <button type="button" :disabled="pending" @click="download()"><Download :size="15" /> Download</button>
            <button v-if="sharing && write" type="button" :disabled="pending" @click="emit('share')">Create public link</button>
            <button v-if="write" type="button" :disabled="pending" @click="confirmation = { kind: 'delete' }">Delete file</button>
          </div>
        </template>
      </template>
      <template v-else>
        <Notice v-if="history.error.value" tone="error">{{ history.error.value }}</Notice>
        <p v-if="history.pending.value" class="muted portal-note" role="status">Loading version history…</p>
        <div v-if="versions" class="portal-table-scroll">
          <table class="portal-table">
            <thead><tr><th scope="col">Version</th><th scope="col">Modified</th><th scope="col">Size</th><th scope="col">Actions</th></tr></thead>
            <tbody>
              <tr v-for="version in versions.versions" :key="version.version_id">
                <th scope="row">{{ version.is_delete_marker ? 'Delete marker' : version.is_latest ? 'Current file' : version.version_id }}</th>
                <td>{{ formatDate(version.last_modified, true) }}</td>
                <td>{{ formatSize(version.size) }}</td>
                <td><div v-if="!version.is_delete_marker && !confirmation" class="portal-inline">
                  <button type="button" :disabled="pending" @click="download(version.version_id)">Download</button>
                  <button v-if="write && versions.can_restore && !version.is_latest" type="button" :disabled="pending" @click="confirmation = { kind: 'restore', version }">Restore</button>
                </div></td>
              </tr>
              <tr v-if="!versions.versions.length"><td colspan="4" class="muted">No versions available. Versioning is {{ versions.versioning_status.toLowerCase() }}.</td></tr>
            </tbody>
          </table>
          <button v-if="versions.next_key_marker" type="button" :disabled="history.pending.value" @click="loadVersions(true)">Load more</button>
        </div>
      </template>
      <div v-if="confirmation" class="portal-panel">
        <p v-if="confirmation.kind === 'delete'" class="portal-note">Delete the current file? Recovery depends on the space’s versioning settings.</p>
        <p v-else class="portal-note">Restore version {{ confirmation.version.version_id }} as the current file?</p>
        <div class="portal-inline">
          <button type="button" :disabled="pending" @click="confirmation = undefined">Cancel</button>
          <button type="button" :disabled="pending || !write" :class="confirmation.kind === 'delete' ? 'danger' : 'primary'" @click="apply">{{ pending ? 'Working…' : 'Confirm' }}</button>
        </div>
      </div>
    </div>
  </Modal>
</template>

<style scoped>
.file-preview { max-height: 320px; max-width: 100%; object-fit: contain; overflow: auto; background: var(--surface-alt); border-radius: 6px; }
pre.file-preview { padding: 12px; }
</style>
