<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ExternalLink, RefreshCw, SlidersHorizontal } from 'lucide-vue-next'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { sharedServiceApi } from '../../connection/api/shared-service'
import { useSharedProfile } from '../../connection/composables/useSharedProfile'
import type { SharedAction } from '../../connection/model/shared-service'
import type { SettingsSection } from '../model/workspace-settings'
import WorkspaceCostPolicies from './WorkspaceCostPolicies.vue'
import WorkspaceResourcePolicies from './WorkspaceResourcePolicies.vue'
import WorkspacePolicyForm from './WorkspacePolicyForm.vue'
import WorkspacePortalPolicies from './WorkspacePortalPolicies.vue'
import WorkspaceReadiness from './WorkspaceReadiness.vue'
import WorkspaceRotation from './WorkspaceRotation.vue'

type Tab = SettingsSection | 'costs' | 'connections' | 'readiness' | 'rotation'
const tabs: { value: Tab; label: string }[] = [
  { value: 'general', label: 'General' }, { value: 'manager', label: 'Manager' }, { value: 'browser', label: 'Browser' },
  { value: 'portal', label: 'Portal' }, { value: 'costs', label: 'Costs' }, { value: 'connections', label: 'Connections' },
  { value: 'quota_notifications', label: 'Quota alerts' }, { value: 'readiness', label: 'Readiness' }, { value: 'rotation', label: 'Key rotation' },
]
const emit = defineEmits<{ connections: [] }>()
const { serviceId, profile, options, pending, error, loadProfiles } = useSharedProfile(status => !!status.session?.access.admin?.available)
const tab = ref<Tab>('general')
const revision = ref(0)
const opening = ref(false)
const actionError = ref('')
const admin = computed(() => !!profile.value?.session?.access.admin?.available)
const browserActions = computed<{ label: string; action: SharedAction }[]>(() => tab.value === 'costs'
  ? [{ label: 'Cost rates and endpoint overrides', action: 'costSettings' }]
  : [{ label: 'Shared connection policies', action: 'sharedConnections' }, { label: 'Endpoint capabilities and credentials', action: 'storageEndpointSettings' }])
const section = computed<SettingsSection | undefined>(() => {
  switch (tab.value) {
    case 'general': case 'manager': case 'browser': case 'portal': case 'quota_notifications': return tab.value
    default: return undefined
  }
})

onMounted(loadProfiles)

async function refresh(): Promise<void> {
  await loadProfiles()
  revision.value++
}

async function open(action: SharedAction): Promise<void> {
  actionError.value = ''
  opening.value = true
  try {
    await sharedServiceApi.open(serviceId.value, action)
  } catch (failure) {
    actionError.value = errorMessage(failure)
  } finally {
    opening.value = false
  }
}
</script>

<template>
  <section class="workspace-page">
    <WorkspaceHeader title="Workspace settings" description="Manage policies, deployment readiness and key rotation for a shared service.">
      <template #actions>
        <button v-if="profile" type="button" :disabled="pending || opening" @click="refresh"><RefreshCw :size="15" /> Refresh</button>
        <button type="button" @click="emit('connections')">Service connections</button>
      </template>
    </WorkspaceHeader>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <EmptyState v-if="!profile && !pending && !error" :icon="SlidersHorizontal" title="Choose a shared service" description="Save a service origin in Connections to manage its settings." />
    <template v-if="profile">
      <div class="settings-sheet">
        <div class="settings-row">
          <AppSelect v-model="serviceId" :options="options" aria-label="Shared service" :disabled="opening" />
          <span class="muted">{{ profile.profile.origin }}</span>
          <span class="chip">{{ profile.session?.principal.email || 'API disconnected' }}</span>
        </div>
      </div>
      <WorkspaceTabs v-model="tab" label="Settings views" :tabs="tabs" />
      <FadeRegion>
        <div :key="`${serviceId}:${tab}:${revision}`" class="settings-view">
          <template v-if="tab === 'costs' || tab === 'connections'">
            <WorkspaceCostPolicies v-if="tab === 'costs' && admin && profile.session?.principal.role === 'ui_superadmin'" :service-id="serviceId" />
            <WorkspaceResourcePolicies v-if="tab === 'connections' && admin" :service-id="serviceId" connections />
            <Notice v-if="actionError" tone="error">{{ actionError }}</Notice>
            <div class="settings-sheet">
              <div v-for="item in browserActions" :key="item.label" class="settings-row">
                <strong>{{ item.label }}</strong>
                <button type="button" :disabled="opening" @click="open(item.action)"><ExternalLink :size="15" /> Open in browser</button>
              </div>
            </div>
          </template>
          <Notice v-else-if="!admin">Authorize the shared service with an Admin token to access this view.</Notice>
          <WorkspaceReadiness v-else-if="tab === 'readiness'" :service-id="serviceId" />
          <WorkspaceRotation v-else-if="tab === 'rotation'" :service-id="serviceId" />
          <template v-else-if="section">
            <WorkspacePolicyForm v-if="profile.session?.principal.role === 'ui_superadmin'" :service-id="serviceId" :section="section" />
            <Notice v-else>Platform settings require a superadmin.</Notice>
            <WorkspaceResourcePolicies v-if="section === 'manager'" :service-id="serviceId" />
            <WorkspacePortalPolicies v-if="section === 'portal'" :service-id="serviceId" />
          </template>
        </div>
      </FadeRegion>
    </template>
  </section>
</template>

<style scoped>
.settings-view { display: grid; gap: 12px; min-width: 0; }
.settings-row { flex-wrap: wrap; }
</style>
