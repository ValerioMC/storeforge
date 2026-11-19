<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ArrowUp, File, Folder, FolderPlus, Upload } from 'lucide-vue-next'
import { open } from '@tauri-apps/plugin-dialog'
import { formatDate, formatSize } from '../../../common/format'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import type { FileView, PortalLocation } from '../model/navigation'
import { fileName, parentFolder, spaceAccess, type FileListing, type PortalSpace, type SpaceOperationResult, type SpaceScope, type TrashListing } from '../model/spaces'
import PortalFileDetails from './PortalFileDetails.vue'

const props = defineProps<{ scope: SpaceScope; space: PortalSpace; location: PortalLocation; cleanup: boolean }>()
const emit = defineEmits<{ navigate: [location: PortalLocation] }>()
const files = usePortalTask<FileListing>()
const trash = usePortalTask<TrashListing>()
const mutation = usePortalTask<boolean>()
const { data: listing } = files
const { data: deleted } = trash
const { pending: writing, error: writeError } = mutation
const prefix = computed(() => props.location.folder ?? '')
const isTrash = computed(() => props.location.view === 'trash')
const access = computed(() => spaceAccess(props.space))
const folders = computed(() => [...new Set(deleted.value?.items.filter(row => row.key.startsWith(prefix.value))
  .map(row => row.key.slice(prefix.value.length).split('/'))
  .filter(parts => parts.length > 1 && !!parts[0]).map(parts => prefix.value + parts[0] + '/') ?? [])])
const trashFiles = computed(() => deleted.value?.items.filter(row => row.key.startsWith(prefix.value) && parentFolder(row.key) === prefix.value) ?? [])
const action = ref<'folder' | 'cleanup' | 'restoreFolder' | 'upload'>()
const folder = ref('')
const confirmation = ref('')
const cleanupPhrase = computed(() => `CLEAN HISTORY ${props.space.name.toUpperCase()}`)
const message = ref('')
const result = ref<SpaceOperationResult>()
const uploadPath = ref('')
const uploadName = computed(() => uploadPath.value.split(/[/\\]/).at(-1) || '')
const listingError = computed(() => isTrash.value ? trash.error.value : files.error.value)
const listingPending = computed(() => isTrash.value ? trash.pending.value : files.pending.value)

watch([prefix, isTrash, () => access.value.browse], () => {
  files.clear()
  trash.clear()
  if (access.value.browse) void refresh()
}, { immediate: true })

async function refresh(): Promise<void> {
  const scope = props.scope
  if (isTrash.value) await trash.run(() => portalApi.trash(scope))
  else await files.run(() => portalApi.files(scope, prefix.value))
}

async function nextPage(): Promise<void> {
  if (isTrash.value) {
    const current = deleted.value
    if (!current?.next_key_marker) return
    await trash.run(async () => {
      const next = await portalApi.trash(props.scope, {
        key_marker: current.next_key_marker ?? undefined, version_id_marker: current.next_version_id_marker ?? undefined,
      })
      return { ...next, items: [...current.items, ...next.items] }
    })
  } else {
    const current = listing.value
    if (!current?.next_continuation_token) return
    await files.run(async () => {
      const next = await portalApi.files(props.scope, prefix.value, current.next_continuation_token ?? undefined)
      return { ...next, objects: [...current.objects, ...next.objects], prefixes: [...new Set([...current.prefixes, ...next.prefixes])] }
    })
  }
}

function navigate(folder: string, object?: string, fileView?: FileView): void {
  emit('navigate', {
    page: 'spaces', serviceId: props.scope.id, projectId: props.scope.project, spaceId: props.scope.space,
    view: isTrash.value ? 'trash' : 'files', folder: folder || undefined, object, fileView,
  })
}

function openAction(next: typeof action.value, prefix = ''): void {
  mutation.clear()
  folder.value = prefix
  confirmation.value = ''
  uploadPath.value = ''
  action.value = next
}

async function chooseUpload(): Promise<void> {
  const selected = await mutation.run(async () => {
    const path = await open({ title: 'Upload file to storage space' })
    if (typeof path !== 'string') return false
    uploadPath.value = path
    return true
  })
  if (selected) action.value = 'upload'
}

async function submit(): Promise<void> {
  const requested = action.value
  if (!requested || !access.value.write || writing.value) return
  message.value = ''
  result.value = undefined
  const applied = await mutation.run(async () => {
    if (requested === 'folder') await portalApi.folder(props.scope, `${prefix.value}${folder.value.trim()}/`)
    if (requested === 'upload') await portalApi.upload(props.scope, `${prefix.value}${uploadName.value}`, uploadPath.value)
    if (requested === 'cleanup') result.value = await portalApi.cleanup(props.scope, confirmation.value)
    if (requested === 'restoreFolder') result.value = await portalApi.restorePrefix(props.scope, folder.value)
    return true
  })
  if (applied) {
    action.value = undefined
    if (requested === 'folder') message.value = 'Folder created.'
    if (requested === 'upload') message.value = 'File uploaded.'
    await refresh()
  }
}
</script>

<template>
  <section class="portal-panel">
    <div class="portal-between">
      <nav class="portal-inline" aria-label="Space folders">
        <button type="button" class="subtle" :disabled="!prefix" @click="navigate(parentFolder(prefix))" aria-label="Parent folder"><ArrowUp :size="16" /></button>
        <button type="button" class="subtle" @click="navigate('')">{{ isTrash ? 'Deleted files' : 'Files' }}</button>
        <span v-if="prefix" class="portal-prefix">{{ prefix }}</span>
      </nav>
      <div v-if="access.write" class="portal-inline">
        <button v-if="!isTrash" type="button" :disabled="writing" @click="openAction('folder')"><FolderPlus :size="15" /> New folder</button>
        <button v-if="!isTrash" type="button" :disabled="writing" @click="chooseUpload"><Upload :size="15" /> Upload file</button>
        <button v-if="isTrash && cleanup && access.managed" type="button" @click="openAction('cleanup')">Clean version history</button>
      </div>
    </div>
    <Notice v-if="!access.browse">File access is unavailable for this space.</Notice>
    <Notice v-if="listingError" tone="error">{{ listingError }}</Notice>
    <Notice v-if="writeError && !action" tone="error">{{ writeError }}</Notice>
    <Notice v-if="message" tone="success">{{ message }}</Notice>
    <div v-if="result">
      <Notice :tone="result.status === 'completed' ? 'success' : 'info'">
        <span v-if="result.restored_objects !== undefined">{{ result.status }} · {{ result.restored_objects }} restored, {{ result.failed_objects }} failed</span>
        <span v-else>{{ result.status }} · {{ result.deleted_versions }} versions and {{ result.deleted_delete_markers }} delete markers removed</span>
      </Notice>
      <ul v-if="result.failures?.length">
        <li v-for="failure in result.failures" :key="failure.key">{{ failure.key }} · {{ failure.detail }}</li>
      </ul>
    </div>
    <p v-if="listingPending" role="status" class="muted portal-note">Loading {{ isTrash ? 'deleted files' : 'files' }}…</p>
    <div v-if="access.browse && ((!isTrash && listing) || (isTrash && deleted))" class="portal-table-scroll">
      <table class="portal-table">
        <thead><tr><th scope="col">Name</th><th scope="col">Size</th><th scope="col">{{ isTrash ? 'Deleted' : 'Modified' }}</th><th v-if="isTrash" scope="col">Actions</th></tr></thead>
        <tbody v-if="!isTrash">
          <tr v-for="directory in listing?.prefixes" :key="directory">
            <th scope="row"><button type="button" class="subtle portal-button-label" @click="navigate(directory)"><Folder :size="16" /> {{ fileName(directory) }}</button></th>
            <td>—</td><td>—</td>
          </tr>
          <tr v-for="file in listing?.objects" :key="file.key">
            <th scope="row"><button type="button" class="subtle portal-button-label" @click="navigate(prefix, file.key, 'details')"><File :size="16" /> {{ fileName(file.key) }}</button></th>
            <td>{{ formatSize(file.size) }}</td><td>{{ formatDate(file.last_modified, true) }}</td>
          </tr>
          <tr v-if="!listing?.objects.length && !listing?.prefixes.length"><td colspan="3" class="muted">This folder is empty.</td></tr>
        </tbody>
        <tbody v-else>
          <tr v-for="directory in folders" :key="directory">
            <th scope="row"><button type="button" class="subtle portal-button-label" @click="navigate(directory)"><Folder :size="16" /> {{ fileName(directory) }}</button></th>
            <td>—</td><td>—</td>
            <td><button v-if="access.write && deleted?.can_restore" type="button" @click="openAction('restoreFolder', directory)">Restore folder</button></td>
          </tr>
          <tr v-for="file in trashFiles" :key="file.key">
            <th scope="row"><button type="button" class="subtle portal-button-label" @click="navigate(prefix, file.key, 'versions')"><File :size="16" /> {{ file.name }}</button></th>
            <td>{{ formatSize(file.size) }}</td><td>{{ formatDate(file.deleted_at, true) }}</td>
            <td><button type="button" @click="navigate(prefix, file.key, 'versions')">History</button></td>
          </tr>
          <tr v-if="!trashFiles.length && !folders.length"><td colspan="4" class="muted">No deleted files in the loaded pages.</td></tr>
        </tbody>
      </table>
      <button v-if="isTrash ? deleted?.next_key_marker : listing?.next_continuation_token" type="button" :disabled="listingPending" @click="nextPage">Load more</button>
    </div>
  </section>

  <PortalFileDetails v-if="location.object && access.browse" :key="location.object"
    :scope="scope" :object-key="location.object" :view="location.fileView ?? 'details'" :write="access.write" :trash="isTrash"
    @close="navigate(prefix)" @view="navigate(prefix, location.object, $event)" @changed="refresh" />

  <Modal v-if="action" :title="action === 'folder' ? 'New folder' : action === 'cleanup' ? 'Clean version history' : action === 'upload' ? 'Upload file' : 'Restore deleted folder'" @close="!writing && (action = undefined)">
    <form class="modal-form" @submit.prevent="submit">
      <label v-if="action === 'folder'" class="portal-control">Folder name <input v-model="folder" :disabled="writing" required pattern="[^/]+" /></label>
      <template v-else-if="action === 'cleanup'">
        <p>This permanently removes eligible historical versions and delete markers. Current files stay available.</p>
        <label class="portal-control">Type {{ cleanupPhrase }} to confirm <input v-model="confirmation" :disabled="writing" required autocomplete="off" /></label>
      </template>
      <p v-else-if="action === 'upload'">Upload <strong>{{ uploadName }}</strong> to {{ prefix || 'the root folder' }}. An existing file with the same name may be replaced.</p>
      <p v-else>Restore the most recent recoverable version of files under <strong>{{ folder }}</strong>.</p>
      <Notice v-if="writeError" tone="error">{{ writeError }}</Notice>
      <footer class="modal-actions">
        <button type="button" :disabled="writing" @click="action = undefined">Cancel</button>
        <button type="submit" :class="action === 'cleanup' ? 'danger' : 'primary'"
          :disabled="writing || !access.write || action === 'cleanup' && confirmation !== cleanupPhrase || action === 'folder' && !folder.trim()">
          {{ writing ? 'Waiting for the service…' : 'Confirm' }}
        </button>
      </footer>
    </form>
  </Modal>
</template>

<style scoped>
.portal-prefix { overflow-wrap: anywhere; color: var(--muted); }
</style>
