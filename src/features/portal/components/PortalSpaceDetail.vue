<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { ArrowLeft, Pencil, RotateCcw, Trash2 } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { formatCount } from '../../monitoring/model/dashboard'
import AppSelect from '../../../components/ui/AppSelect.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import type { PortalLocation, SpaceView } from '../model/navigation'
import { spaceAccess, type PortalSpace, type PortalState, type SpaceIcon, type SpaceScope, type SpaceStatistics } from '../model/spaces'
import PortalFiles from './PortalFiles.vue'
import PortalSpaceSettings from './PortalSpaceSettings.vue'

const props = defineProps<{ scope: SpaceScope; location: PortalLocation; state?: PortalState }>()
const emit = defineEmits<{ navigate: [location: PortalLocation]; back: []; changed: []; deleted: [] }>()
const resource = usePortalTask<PortalSpace>()
const stats = usePortalTask<SpaceStatistics>()
const mutation = usePortalTask<boolean>()
const { data: space } = resource
const { data: statistics } = stats
const { pending, error } = mutation
const view = computed(() => props.location.view ?? 'files')
const access = computed(() => space.value ? spaceAccess(space.value) : undefined)
const tabs = [
  { value: 'files', label: 'Files' }, { value: 'trash', label: 'Deleted files' },
  { value: 'statistics', label: 'Statistics' }, { value: 'settings', label: 'Settings' },
] as const
const iconOptions: { value: SpaceIcon | 'default'; label: string }[] = [
  { value: 'default', label: 'Default' }, { value: 'bucket', label: 'Bucket' }, { value: 'folder', label: 'Folder' },
  { value: 'archive', label: 'Archive' }, { value: 'database', label: 'Database' }, { value: 'media', label: 'Media' },
]
const action = ref<'edit' | 'archive' | 'ownership' | 'delete'>()
const confirmation = ref('')
const name = ref('')
const description = ref('')
const icon = ref<SpaceIcon | 'default'>('default')
const message = ref('')
const projectManager = computed(() => props.state?.can_manage_portal_users)

onMounted(reload)
watch([view, () => props.state?.bucket_usage_stats_enabled], ([next, enabled]) => {
  if (next === 'statistics' && enabled) void stats.run(() => portalApi.statistics(props.scope))
}, { immediate: true })

function reload(): Promise<PortalSpace | undefined> {
  return resource.run(() => portalApi.space(props.scope))
}

function selectView(next: SpaceView): void {
  emit('navigate', {
    page: 'spaces', serviceId: props.scope.id, projectId: props.scope.project, spaceId: props.scope.space,
    view: next, folder: props.location.folder,
  })
}

function openAction(next: typeof action.value): void {
  mutation.clear()
  confirmation.value = ''
  name.value = space.value?.name ?? ''
  description.value = space.value?.description ?? ''
  icon.value = space.value?.icon.preset ?? 'default'
  action.value = next
}

async function editSpace(current: PortalSpace): Promise<void> {
  const draft = { ...(current.name_editable ? { name: name.value.trim() } : {}), description: description.value.trim() }
  if (draft.name !== undefined && draft.name !== current.name || draft.description !== (current.description ?? '')) {
    space.value = await portalApi.edit(props.scope, draft)
  }
  if (projectManager.value && icon.value !== (current.icon.preset ?? 'default')) {
    await portalApi.setIcon(props.scope, icon.value === 'default' ? null : icon.value)
  }
}

async function submit(): Promise<void> {
  const current = space.value
  const requested = action.value
  if (!current || !requested) return
  const applied = await mutation.run(async () => {
    if (requested === 'edit') await editSpace(current)
    if (requested === 'archive') space.value = await portalApi.archive(props.scope, current.status !== 'Archived')
    if (requested === 'ownership') space.value = await portalApi.takeOwnership(props.scope)
    if (requested === 'delete') await portalApi.delete(props.scope, confirmation.value)
    return true
  })
  if (!applied) return
  action.value = undefined
  if (requested === 'delete') return emit('deleted')
  message.value = requested === 'archive' ? current.status === 'Archived' ? 'Space restored.' : 'Space archived.' : 'Space updated.'
  await reload()
  emit('changed')
}
</script>

<template>
  <div class="space-detail">
    <div class="portal-between">
      <button type="button" class="subtle" @click="emit('back')"><ArrowLeft :size="15" /> All spaces</button>
      <div v-if="space && access" class="portal-inline">
        <button v-if="access.managed && space.status !== 'Archived'" type="button" @click="openAction('edit')"><Pencil :size="14" /> Edit space</button>
        <button v-if="access.takeOwnership" type="button" @click="openAction('ownership')">Take ownership</button>
        <button v-if="access.managed" type="button" @click="openAction('archive')">
          <RotateCcw v-if="space.status === 'Archived'" :size="14" /> {{ space.status === 'Archived' ? 'Restore space' : 'Archive space' }}
        </button>
        <button v-if="access.delete" type="button" @click="openAction('delete')"><Trash2 :size="14" /> Delete space</button>
      </div>
    </div>
    <Notice v-if="resource.error.value" tone="error">{{ resource.error.value }}</Notice>
    <p v-if="resource.pending.value && !space" class="muted portal-note" role="status">Loading space…</p>
    <template v-if="space && access">
      <div class="portal-panel">
        <div class="portal-between">
          <h2 class="space-name">{{ space.name }}</h2>
          <div class="portal-inline"><span class="chip">{{ space.role }}</span><span class="chip">{{ space.status }}</span></div>
        </div>
        <p v-if="space.description" class="muted portal-note">{{ space.description }}</p>
        <Notice v-if="space.status === 'Archived'">This space is archived. Restore it to make changes to its files.</Notice>
        <Notice v-if="message" tone="success">{{ message }}</Notice>
      </div>
      <WorkspaceTabs :model-value="view" label="Space views" :tabs="tabs" @update:model-value="selectView" />
      <FadeRegion>
        <div :key="`space-view:${view === 'files' || view === 'trash' ? 'browser' : view}`" class="space-detail">
          <PortalFiles v-if="view === 'files' || view === 'trash'" :scope="scope" :space="space" :location="location"
            :cleanup="Boolean(state?.storage_space_version_cleanup_enabled)" @navigate="emit('navigate', $event)" />
          <PortalSpaceSettings v-else-if="view === 'settings'" :scope="scope" :editable="access.managed && access.write" />
          <section v-else class="portal-panel">
            <h2>Space statistics</h2>
            <dl class="portal-statistics">
              <div><dt>Storage used</dt><dd>{{ formatSize(space.used_bytes) }}</dd></div>
              <div><dt>Objects</dt><dd>{{ formatCount(space.object_count) }}</dd></div>
              <div><dt>Storage quota</dt><dd>{{ formatSize(space.quota_max_size_bytes) }}</dd></div>
              <div><dt>People with access</dt><dd>{{ formatCount(space.collaborator_count) }}</dd></div>
            </dl>
            <Notice v-if="stats.error.value" tone="error">{{ stats.error.value }}</Notice>
            <p v-else-if="stats.pending.value" role="status" class="muted portal-note">Loading the latest statistics…</p>
            <template v-else-if="statistics?.snapshot">
              <dl class="portal-statistics">
                <div><dt>Version storage</dt><dd>{{ formatSize(statistics.snapshot.total_bytes) }}</dd></div>
                <div><dt>Current versions</dt><dd>{{ formatCount(statistics.snapshot.current_version_count) }}</dd></div>
              </dl>
              <p class="muted portal-note">Collected {{ formatDate(statistics.snapshot.calculated_at, true) }}</p>
            </template>
          </section>
        </div>
      </FadeRegion>
      <Modal v-if="action" :title="action === 'edit' ? 'Edit storage space' : action === 'ownership' ? 'Take ownership' : action === 'delete' ? 'Delete empty space' : space.status === 'Archived' ? 'Restore space' : 'Archive space'" @close="!pending && (action = undefined)">
        <form class="modal-form" @submit.prevent="submit">
          <template v-if="action === 'edit'">
            <label class="portal-control">Space name <input v-model="name" required maxlength="255" :disabled="!space.name_editable || pending" /></label>
            <label class="portal-control">Description <textarea v-model="description" maxlength="2000" :disabled="pending" /></label>
            <AppSelect v-if="projectManager" v-model="icon" :options="iconOptions" aria-label="Space icon" :disabled="pending" />
          </template>
          <template v-else-if="action === 'delete'">
            <p>The service checks for files, historical versions and delete markers before removing the space.</p>
            <label class="portal-control">Type {{ space.name }} to confirm <input v-model="confirmation" :disabled="pending" required autocomplete="off" /></label>
          </template>
          <p v-else-if="action === 'ownership'">Take ownership of {{ space.name }} for your account.</p>
          <p v-else-if="space.status === 'Archived'">Restore file access to {{ space.name }}.</p>
          <p v-else>Archive {{ space.name }} and suspend file changes until the space is restored.</p>
          <Notice v-if="error" tone="error">{{ error }}</Notice>
          <footer class="modal-actions">
            <button type="button" :disabled="pending" @click="action = undefined">Cancel</button>
            <button type="submit" :class="action === 'delete' ? 'danger' : 'primary'" :disabled="pending || action === 'delete' && confirmation !== space.name">{{ pending ? 'Working…' : 'Confirm' }}</button>
          </footer>
        </form>
      </Modal>
    </template>
  </div>
</template>

<style scoped>
.space-detail { display: grid; min-width: 0; gap: 12px; }
.space-name { font-size: 21px; overflow-wrap: anywhere; }
</style>
