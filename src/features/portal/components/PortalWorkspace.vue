<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { Cable, RefreshCw } from 'lucide-vue-next'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import { useSharedProfile } from '../../connection/composables/useSharedProfile'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import { portalPages, type PortalLocation, type PortalPage } from '../model/navigation'
import type { PortalProject, PortalScope } from '../model/spaces'
import PortalCollaborators from './PortalCollaborators.vue'
import PortalKeys from './PortalKeys.vue'
import PortalMembershipRequests from './PortalMembershipRequests.vue'
import PortalDashboard from './PortalDashboard.vue'
import PortalSpaces from './PortalSpaces.vue'

const props = defineProps<{ location: PortalLocation }>()
const emit = defineEmits<{ navigate: [location: PortalLocation]; connections: [] }>()
const services = useSharedProfile(status => !!status.session?.access.portal.available)
const { serviceId, profile, options, pending, error } = services
const projects = usePortalTask<PortalProject[]>()
const revision = ref(0)
const tabs = (Object.keys(portalPages) as PortalPage[]).map(value => ({ value, label: portalPages[value] }))
const projectOptions = computed(() => (projects.data.value ?? []).map(project => ({ value: String(project.id), label: project.name })))
const project = computed(() => projects.data.value?.find(project => project.id === props.location.projectId))
const eligible = computed(() => !!profile.value?.session?.access.portal.available)
const scope = computed<PortalScope | undefined>(() => eligible.value && project.value && serviceId.value === props.location.serviceId
  ? { id: serviceId.value, project: project.value.id } : undefined)

onMounted(refresh)
watch(() => props.location.serviceId, id => {
  if (id && serviceId.value !== id) {
    serviceId.value = id
    void loadProjects()
  }
})

async function refresh(): Promise<void> {
  await services.loadProfiles()
  if (props.location.serviceId) serviceId.value = props.location.serviceId
  await loadProjects()
  revision.value++
}

async function loadProjects(): Promise<void> {
  projects.clear()
  if (!eligible.value) return
  const id = serviceId.value
  const list = await projects.run(() => portalApi.projects(id))
  const first = list?.[0]
  if (first && (!props.location.serviceId || !props.location.projectId)) {
    emit('navigate', { page: props.location.page, serviceId: id, projectId: first.id })
  }
}

function selectService(id: string): void {
  emit('navigate', { page: props.location.page, serviceId: id })
}

function selectProject(id: string): void {
  emit('navigate', { page: props.location.page, serviceId: serviceId.value, projectId: Number(id) })
}

function selectPage(page: PortalPage): void {
  emit('navigate', { page, serviceId: serviceId.value || undefined, projectId: project.value?.id })
}

function openPerson(personId?: number): void {
  if (scope.value) emit('navigate', { page: 'collaborators', serviceId: scope.value.id, projectId: scope.value.project, personId })
}

function openSpace(spaceId: string): void {
  if (scope.value) emit('navigate', { page: 'spaces', serviceId: scope.value.id, projectId: scope.value.project, spaceId })
}
</script>

<template>
  <section class="workspace-page portal-page">
    <WorkspaceHeader :title="location.page === 'overview' ? 'Portal' : portalPages[location.page]" :description="project?.name">
      <template #actions>
        <button type="button" :disabled="pending || projects.pending.value" @click="refresh"><RefreshCw :size="15" /> Refresh</button>
        <button type="button" @click="emit('connections')">Service connections</button>
      </template>
    </WorkspaceHeader>

    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <div v-if="options.length" class="portal-context settings-sheet">
      <AppSelect :model-value="serviceId" :options="options" aria-label="Portal service" @update:model-value="selectService" />
      <AppSelect v-if="eligible" :model-value="project ? String(project.id) : ''" :options="projectOptions" :disabled="projects.pending.value"
        aria-label="Portal project" placeholder="Select a project" @update:model-value="selectProject" />
      <span v-if="project" class="chip">{{ project.portal_role === 'portal_manager' ? 'Portal Manager' : 'Portal member' }}</span>
    </div>
    <WorkspaceTabs :model-value="location.page" label="Portal views" :tabs="tabs" @update:model-value="selectPage" />

    <FadeRegion>
      <div :key="scope ? JSON.stringify([scope.id, scope.project, location.page, revision]) : 'portal-unavailable'" class="portal-content">
        <Notice v-if="projects.error.value" tone="error">{{ projects.error.value }}</Notice>
        <p v-else-if="pending || projects.pending.value" role="status" class="muted">Loading Portal projects…</p>
        <EmptyState v-else-if="!eligible" :icon="Cable" title="Connect a Portal service"
          description="Authorize a saved shared service to open projects you can access." />
        <Notice v-else-if="!scope">{{ location.projectId ? 'This project is unavailable on the selected service.' : 'No Portal projects are assigned to this account.' }}</Notice>
        <PortalDashboard v-else-if="location.page === 'overview'" :scope="scope" :project="project!" @open="openSpace" @spaces="selectPage('spaces')" />
        <PortalCollaborators v-else-if="location.page === 'collaborators'" :scope="scope" :person="location.personId"
          :manager="project?.portal_role === 'portal_manager'" @person="openPerson" @space="openSpace" @requests="selectPage('requests')" />
        <PortalKeys v-else-if="location.page === 'keys'" :scope="scope" />
        <PortalMembershipRequests v-else-if="location.page === 'requests'" :scope="scope" />
        <PortalSpaces v-else :scope="scope" :location="location" @navigate="emit('navigate', $event)" @open="openSpace" @spaces="selectPage('spaces')" />
      </div>
    </FadeRegion>
  </section>
</template>

<style>
.portal-page { --portal-inset: 16px; }
.portal-content { display: grid; gap: 14px; min-width: 0; }
.portal-context { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; padding: 10px var(--portal-inset); }
.portal-context > .select-field { min-width: 180px; }
.portal-panel { display: grid; gap: 12px; min-width: 0; padding: var(--portal-inset); border: 1px solid var(--line); border-radius: 10px; background: var(--surface); align-content: start; }
.portal-panel > h2, .portal-panel h3 { margin: 0; }
.portal-panel > h2 { font-size: 15px; }
.portal-grid { display: grid; grid-template-columns: minmax(0, 1.35fr) minmax(0, 1fr); gap: 12px; }
.portal-inline { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.portal-between { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; }
.portal-list { display: grid; gap: 0; margin: 0; padding: 0; list-style: none; }
.portal-list > li { border-bottom: 1px solid var(--line); padding: 10px 0; min-width: 0; overflow-wrap: anywhere; }
.portal-list > li:last-child { border-bottom: 0; }
.portal-button-label { text-align: left; overflow-wrap: anywhere; white-space: normal; }
.portal-table { width: 100%; min-width: 430px; border-collapse: collapse; text-align: left; }
.portal-table th, .portal-table td { border-bottom: 1px solid var(--line); padding: 9px 10px; overflow-wrap: anywhere; }
.portal-table th { font-weight: 600; }
.portal-table tbody tr:last-child td { border-bottom: 0; }
.portal-table-scroll { overflow: auto; }
.portal-control { display: grid; gap: 6px; }
.portal-control textarea { min-height: 70px; }
.portal-statistics { display: grid; gap: 14px; grid-template-columns: repeat(auto-fit, minmax(115px, 1fr)); margin: 0; }
.portal-statistics dt { color: var(--muted); font-size: 12px; }
.portal-statistics dd { margin: 5px 0 0; font-size: 19px; font-weight: 650; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.portal-note { margin: 0; overflow-wrap: anywhere; }
@media (max-width: 920px) { .portal-grid { grid-template-columns: minmax(0, 1fr); } }
</style>
