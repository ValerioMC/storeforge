<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Archive, Box, Database, Folder, Image, Plus } from 'lucide-vue-next'
import { formatSize } from '../../../common/format'
import { formatCount } from '../../monitoring/model/dashboard'
import AppSelect from '../../../components/ui/AppSelect.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Notice from '../../../components/ui/Notice.vue'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import type { PortalLocation } from '../model/navigation'
import type { PortalScope, PortalSpace, PortalState } from '../model/spaces'
import CreatePortalSpace from './CreatePortalSpace.vue'
import PortalSpaceDetail from './PortalSpaceDetail.vue'

const props = defineProps<{ scope: PortalScope; location: PortalLocation }>()
const emit = defineEmits<{ navigate: [location: PortalLocation]; open: [id: string]; spaces: [] }>()
const spaces = usePortalTask<PortalSpace[]>()
const state = usePortalTask<PortalState>()
const { data: portalState } = state
const search = ref('')
const status = ref('open')
const creating = ref<'new' | 'import'>()
const icons = { bucket: Box, folder: Folder, archive: Archive, database: Database, media: Image }
const filters = [{ value: 'open', label: 'Current spaces' }, { value: 'archived', label: 'Archived spaces' }, { value: 'all', label: 'All spaces' }]
const visible = computed(() => {
  const query = search.value.trim().toLowerCase()
  return (spaces.data.value ?? []).filter(space =>
    (status.value === 'all' || (space.status === 'Archived') === (status.value === 'archived')) &&
    (!query || [space.name, space.description, space.owner_label].some(text => text?.toLowerCase().includes(query))))
})
const canCreate = computed(() => !!(portalState.value?.can_create_private_storage_spaces || portalState.value?.can_create_team_storage_spaces))

onMounted(() => {
  void reload()
  void state.run(() => portalApi.state(props.scope))
})

function reload(): Promise<PortalSpace[] | undefined> {
  return spaces.run(() => portalApi.spaces(props.scope))
}

function created(space: PortalSpace): void {
  creating.value = undefined
  emit('open', space.id)
  void reload()
}

function deleted(): void {
  emit('spaces')
  void reload()
}
</script>

<template>
  <PortalSpaceDetail v-if="location.spaceId"
    :key="location.spaceId" :scope="{ ...scope, space: location.spaceId }" :location="location" :state="portalState"
    @navigate="emit('navigate', $event)" @back="emit('spaces')" @changed="reload" @deleted="deleted" />
  <div v-else class="portal-spaces">
    <div class="portal-between">
      <div class="portal-inline">
        <input v-model="search" type="search" aria-label="Find a storage space" placeholder="Find a storage space" />
        <AppSelect v-model="status" :options="filters" aria-label="Space status" />
      </div>
      <div v-if="canCreate || portalState?.can_manage_buckets" class="portal-inline">
        <button v-if="portalState?.can_manage_buckets && canCreate" type="button" @click="creating = 'import'">Import bucket</button>
        <button v-if="canCreate" type="button" class="primary" @click="creating = 'new'"><Plus :size="15" /> Create space</button>
      </div>
    </div>
    <Notice v-if="state.error.value" tone="error">{{ state.error.value }}</Notice>
    <Notice v-if="spaces.error.value" tone="error">{{ spaces.error.value }}</Notice>
    <p v-if="spaces.pending.value && !spaces.data.value" role="status" class="muted portal-note">Loading storage spaces…</p>
    <EmptyState v-else-if="spaces.data.value && !visible.length" :icon="Folder" title="No matching spaces"
      description="Change the search or space status to show more results." />
    <div v-else-if="visible.length" class="portal-panel portal-table-scroll">
      <table class="portal-table">
        <thead><tr><th scope="col">Space</th><th scope="col">Access</th><th scope="col">Storage</th><th scope="col">Objects</th><th scope="col">Status</th></tr></thead>
        <tbody>
          <tr v-for="space in visible" :key="space.id">
            <th scope="row">
              <button type="button" class="subtle portal-button-label" @click="emit('open', space.id)">
                <component :is="icons[space.icon.preset ?? 'bucket']" :size="16" /> {{ space.name }}
              </button>
              <small v-if="space.description" class="muted description">{{ space.description }}</small>
            </th>
            <td>{{ space.role }} <small v-if="space.owner_label" class="muted description">{{ space.owner_label }}</small></td>
            <td>{{ formatSize(space.used_bytes) }}</td><td>{{ formatCount(space.object_count) }}</td><td><span class="chip">{{ space.status }}</span></td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
  <CreatePortalSpace v-if="creating && portalState" :scope="scope" :state="portalState" :importing="creating === 'import'" @close="creating = undefined" @created="created" />
</template>

<style scoped>
.portal-spaces { display: grid; gap: 12px; }
.description { display: block; font-weight: normal; margin-top: 3px; }
</style>
