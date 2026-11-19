<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { ClipboardCheck, ExternalLink, RefreshCw } from 'lucide-vue-next'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { sharedServiceApi } from '../../connection/api/shared-service'
import { useSharedProfile } from '../../connection/composables/useSharedProfile'
import { operationsApi } from '../api/operational-events'
import type { NotificationInbox } from '../model/operational-events'
import OperationalCollections from './OperationalCollections.vue'
import OperationalInbox from './OperationalInbox.vue'
import OperationalWebhooks from './OperationalWebhooks.vue'
import SharedAuditHistory from './SharedAuditHistory.vue'

const props = defineProps<{ manager?: boolean }>()
const emit = defineEmits<{ connections: [] }>()
type Tab = 'manager' | 'admin' | 'inbox' | 'collections' | 'webhooks' | 'provider'
const tab = ref<Tab>(props.manager ? 'manager' : 'admin')
const { serviceId, profile, options, pending, error, loadProfiles } = useSharedProfile(status =>
  props.manager ? !!status.session?.access.manager?.available : !!status.session?.access.admin?.available)
const admin = computed(() => !!profile.value?.session?.access.admin?.available)
const managerAccess = computed(() => !!profile.value?.session?.access.manager?.available)
const superadmin = computed(() => profile.value?.session?.principal.role === 'ui_superadmin')
const inbox = ref<NotificationInbox>()
const inboxError = ref('')
const providerError = ref('')
const revision = ref(0)
const tabs = computed<{ value: Tab; label: string }[]>(() => [
  { value: 'manager', label: 'Manager history' }, { value: 'admin', label: 'Admin audit' },
  { value: 'inbox', label: inbox.value?.unread_count ? `Inbox (${inbox.value.unread_count})` : 'Inbox' },
  { value: 'collections', label: 'Collections' }, { value: 'webhooks', label: 'Webhooks' },
  { value: 'provider', label: 'Provider access' },
])

onMounted(loadProfiles)
watch(serviceId, () => {
  inbox.value = undefined
  providerError.value = ''
  void loadInbox()
})

async function refresh(): Promise<void> {
  await loadProfiles()
  revision.value++
  await loadInbox()
}

async function openProviderLogs(): Promise<void> {
  providerError.value = ''
  try {
    await sharedServiceApi.open(serviceId.value, 'providerAccessLogs')
  } catch (failure) {
    providerError.value = errorMessage(failure)
  }
}

async function loadInbox(): Promise<void> {
  const id = serviceId.value
  inboxError.value = ''
  if (!profile.value?.session) {
    inbox.value = undefined
    return
  }
  try {
    const result = await operationsApi.inbox(id)
    if (id === serviceId.value) inbox.value = result
  } catch (failure) {
    if (id === serviceId.value) inboxError.value = errorMessage(failure)
  }
}
</script>

<template>
  <div class="shared-activity">
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <EmptyState v-if="!profile && !pending && !error" :icon="ClipboardCheck" title="Choose a shared service" description="Connect a shared service to read its activity.">
      <button type="button" @click="emit('connections')">Service connections</button>
    </EmptyState>
    <template v-if="profile">
      <div class="settings-sheet">
        <div class="settings-row">
          <AppSelect v-model="serviceId" :options="options" aria-label="Activity service" />
          <span class="muted">{{ profile.profile.origin }}</span>
          <button type="button" :disabled="pending" @click="refresh"><RefreshCw :size="15" /> Refresh service</button>
        </div>
      </div>
      <WorkspaceTabs v-if="!manager" v-model="tab" label="Shared service activity" :tabs="tabs" />
      <FadeRegion>
        <div :key="`${serviceId}:${tab}:${revision}`" class="shared-activity">
          <OperationalWebhooks v-if="tab === 'webhooks'" :service-id="serviceId" :authorized="admin && superadmin" />
          <div v-else-if="tab === 'provider'" class="settings-sheet">
            <Notice v-if="providerError" tone="error">{{ providerError }}</Notice>
            <div class="settings-row">
              <div class="settings-description">
                <strong>S3 provider access logs</strong><p>Available in Portal to project managers when logging is enabled.</p>
              </div>
              <button type="button" @click="openProviderLogs"><ExternalLink :size="15" /> Open provider logs</button>
            </div>
          </div>
          <Notice v-else-if="!profile.session">Authorize the service in Connections to access its records.</Notice>
          <template v-else-if="tab === 'inbox'">
            <Notice v-if="inboxError" tone="error">{{ inboxError }}</Notice>
            <OperationalInbox :service-id="serviceId" :inbox="inbox" :admin="admin" :superadmin="superadmin" @refresh="loadInbox" />
          </template>
          <Notice v-else-if="tab === 'manager' && !managerAccess">The service token requires Manager access for context history.</Notice>
          <Notice v-else-if="tab !== 'manager' && !admin">The service token requires Admin access for this view.</Notice>
          <OperationalCollections v-else-if="tab === 'collections'" :service-id="serviceId" />
          <SharedAuditHistory v-else :service-id="serviceId" :scope="tab" />
        </div>
      </FadeRegion>
    </template>
  </div>
</template>

<style scoped>
.shared-activity { display: grid; gap: 10px; min-width: 0; }
.settings-row { flex-wrap: wrap; }
</style>
