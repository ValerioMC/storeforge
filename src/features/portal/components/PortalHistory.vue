<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { Download, RefreshCw } from 'lucide-vue-next'
import { save } from '@tauri-apps/plugin-dialog'
import { formatDate, formatSize } from '../../../common/format'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import { governanceApi } from '../api/governance'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import { localDate, type AccessLogField, type AccessLogQuery, type ProviderAccessLog, type ProviderAccessLogPage } from '../model/governance'
import type { PortalActivity, PortalScope, PortalSpace, PortalState } from '../model/spaces'

const props = defineProps<{ scope: PortalScope; manager: boolean }>()
const policy = usePortalTask<PortalState>()
const spaces = usePortalTask<PortalSpace[]>()
const activity = usePortalTask<PortalActivity[]>()
const logs = usePortalTask<ProviderAccessLogPage>()
const exporting = usePortalTask<boolean>()
const tab = ref<'activity' | 'access'>('activity')
const { data: history } = activity
const { data: access } = logs
const space = ref('')
const filter = ref('')
const logField = ref<AccessLogField>('path')
const logFilter = ref('')
const date = ref(localDate())
const appliedLogs = ref<Omit<AccessLogQuery, 'offset'>>()
const selected = ref<PortalActivity>()
const selectedLog = ref<ProviderAccessLog>()
const confirmExport = ref(false)
const from = ref(localDate())
const to = ref(localDate())
const { data: exported, pending: saving, error: exportError } = exporting
const canViewLogs = computed(() => props.manager && !!policy.data.value?.server_access_logging_enabled)
const tabs = computed<{ value: 'activity' | 'access'; label: string }[]>(() => [
  { value: 'activity', label: 'Governance history' },
  ...(canViewLogs.value ? [{ value: 'access' as const, label: 'Provider access logs' }] : []),
])
const spaceOptions = computed(() => [
  { value: '', label: 'All spaces' }, ...(spaces.data.value ?? []).map(space => ({ value: space.id, label: space.name })),
])
const logFields: { value: AccessLogField; label: string }[] = [
  { value: 'path', label: 'Object path' }, { value: 'action', label: 'Operation' }, { value: 'space', label: 'Space' },
  { value: 'identity', label: 'Requester' }, { value: 'result', label: 'Result' },
]
const visible = computed(() => {
  const term = filter.value.trim().toLowerCase()
  return (history.value ?? []).filter(event => !term || [event.action, event.actor, event.target, event.status].some(value => value.toLowerCase().includes(term)))
})

onMounted(() => {
  if (props.manager) void policy.run(() => portalApi.state(props.scope))
  void spaces.run(() => portalApi.spaces(props.scope))
})
watch([space, tab], () => {
  if (tab.value === 'activity') {
    activity.clear()
    void activity.run(() => governanceApi.history(props.scope, space.value || undefined))
  } else {
    logs.clear()
    void applyLogFilters()
  }
}, { immediate: true })

async function applyLogFilters(): Promise<void> {
  if (!date.value) return
  const value = logFilter.value.trim()
  appliedLogs.value = {
    date: date.value, space_id: space.value || undefined, timezone_offset_minutes: timezone(date.value),
    filter: value ? { field: logField.value, value } : undefined,
  }
  await fetchLogs(0)
}

async function fetchLogs(offset: number): Promise<void> {
  if (!canViewLogs.value || !appliedLogs.value) return
  const query = { ...appliedLogs.value, offset }
  await logs.run(() => governanceApi.logs(props.scope, query))
}

function timezone(day: string): number {
  return -new Date(`${day}T12:00:00`).getTimezoneOffset()
}

function openExport(): void {
  exporting.clear()
  from.value = date.value
  to.value = date.value
  confirmExport.value = true
}

async function exportLogs(): Promise<void> {
  if (!canViewLogs.value || saving.value || from.value > to.value) return
  const query = {
    date_from: from.value, date_to: to.value, space_id: space.value || undefined, timezone_offset_minutes: timezone(from.value),
  }
  const result = await exporting.run(async () => {
    const destination = await save({ defaultPath: `portal-access-${query.date_from}.log`, title: 'Export provider access logs' })
    if (!destination) return false
    await governanceApi.exportLogs(props.scope, query, destination)
    return true
  })
  if (result) confirmExport.value = false
}
</script>

<template>
  <WorkspaceTabs v-model="tab" label="History views" :tabs="tabs" />
  <Notice v-if="policy.error.value" tone="error">{{ policy.error.value }}</Notice>
  <Notice v-if="spaces.error.value" tone="error">{{ spaces.error.value }}</Notice>
  <section v-if="tab === 'activity'" class="portal-panel">
    <div class="portal-between"><h2>Project activity</h2>
      <button type="button" :disabled="activity.pending.value" @click="activity.run(() => governanceApi.history(scope, space || undefined))"><RefreshCw :size="14" /> Refresh</button>
    </div>
    <div class="portal-inline">
      <input v-model="filter" type="search" aria-label="Search project activity" placeholder="Action, actor or status" />
      <AppSelect v-model="space" :options="spaceOptions" aria-label="Activity space" />
    </div>
    <Notice v-if="activity.error.value" tone="error">{{ activity.error.value }}</Notice>
    <p v-if="activity.pending.value" class="muted portal-note" role="status">Loading project history…</p>
    <div v-if="history" class="portal-table-scroll">
      <table class="portal-table">
        <thead><tr><th scope="col">Action</th><th scope="col">Actor</th><th scope="col">Status</th><th scope="col">Time</th></tr></thead>
        <tbody>
          <tr v-for="event in visible" :key="event.id">
            <th scope="row"><button type="button" class="subtle portal-button-label" @click="selected = event">{{ event.action }}</button>
              <div class="muted event-target">{{ event.target }}</div>
            </th>
            <td>{{ event.actor }}</td><td><span class="chip">{{ event.status }}</span></td><td>{{ formatDate(event.created_at, true) }}</td>
          </tr>
          <tr v-if="!visible.length"><td colspan="4" class="muted">No matching activity.</td></tr>
        </tbody>
      </table>
    </div>
  </section>
  <section v-else-if="canViewLogs" class="portal-panel">
    <div class="portal-between"><h2>Provider access logs</h2>
      <button type="button" :disabled="saving" @click="openExport"><Download :size="14" /> Export raw logs</button>
    </div>
    <form class="portal-inline" @submit.prevent="applyLogFilters">
      <label class="portal-inline">Date <input v-model="date" type="date" required /></label>
      <AppSelect v-model="space" :options="spaceOptions" aria-label="Access log space" />
      <AppSelect v-model="logField" :options="logFields" aria-label="Access log field" />
      <input v-model="logFilter" type="search" aria-label="Access log filter value" />
      <button type="submit" :disabled="logs.pending.value">Apply filters</button>
    </form>
    <Notice v-if="logs.error.value" tone="error">{{ logs.error.value }}</Notice>
    <Notice v-if="exported" tone="success">Provider logs exported.</Notice>
    <p v-if="logs.pending.value" role="status" class="muted portal-note">Loading provider logs…</p>
    <div v-if="access" class="portal-table-scroll">
      <table class="portal-table">
        <thead><tr><th scope="col">Operation</th><th scope="col">Space</th><th scope="col">Identity</th><th scope="col">Result</th><th scope="col">Time</th></tr></thead>
        <tbody>
          <tr v-for="entry in access.entries" :key="entry.id">
            <th scope="row"><button type="button" class="subtle portal-button-label" @click="selectedLog = entry">{{ entry.operation }}</button></th>
            <td>{{ entry.storage_space_name || entry.bucket_name }}</td><td>{{ entry.requester_identity?.label || entry.requester || '—' }}</td>
            <td>{{ entry.status_code ?? '—' }} {{ entry.error_code }}</td><td>{{ formatDate(entry.timestamp, true) }}</td>
          </tr>
          <tr v-if="!access.entries.length"><td colspan="5" class="muted">No provider log records match the filters.</td></tr>
        </tbody>
      </table>
    </div>
    <div v-if="access && access.total > access.limit" class="portal-inline">
      <button type="button" :disabled="logs.pending.value || !access.offset" @click="fetchLogs(Math.max(0, access.offset - access.limit))">Previous</button>
      <span>{{ access.offset + 1 }}–{{ Math.min(access.offset + access.limit, access.total) }} of {{ access.total }}</span>
      <button type="button" :disabled="logs.pending.value || access.offset + access.limit >= access.total" @click="fetchLogs(access.offset + access.limit)">Next</button>
    </div>
  </section>

  <Modal v-if="selected" title="Project activity" drawer @close="selected = undefined">
    <div class="modal-form"><dl class="setup-summary">
      <div><dt>Action</dt><dd>{{ selected.action }}</dd></div><div><dt>Actor</dt><dd>{{ selected.actor }}</dd></div>
      <div><dt>Target</dt><dd>{{ selected.target }}</dd></div><div><dt>Status</dt><dd>{{ selected.status }}</dd></div>
      <div><dt>Recorded</dt><dd>{{ formatDate(selected.created_at, true) }}</dd></div>
    </dl></div>
  </Modal>
  <Modal v-if="selectedLog" title="Provider access record" drawer @close="selectedLog = undefined">
    <div class="modal-form"><dl class="setup-summary">
      <div><dt>Time</dt><dd>{{ formatDate(selectedLog.timestamp, true) }}</dd></div>
      <div><dt>Operation</dt><dd>{{ selectedLog.operation }}</dd></div>
      <div><dt>Object</dt><dd>{{ selectedLog.object_key ?? '—' }}</dd></div>
      <div><dt>Requester</dt><dd>{{ selectedLog.requester_identity?.label || selectedLog.requester || '—' }}</dd></div>
      <div><dt>Client IP</dt><dd>{{ selectedLog.client_ip ?? '—' }}</dd></div>
      <div><dt>Result</dt><dd>{{ selectedLog.status_code ?? '—' }} {{ selectedLog.error_code }}</dd></div>
      <div><dt>Bytes sent</dt><dd>{{ formatSize(selectedLog.bytes_sent) }}</dd></div>
    </dl></div>
  </Modal>
  <Modal v-if="confirmExport" title="Export provider access logs" @close="!saving && (confirmExport = false)">
    <form class="modal-form" @submit.prevent="exportLogs">
      <label class="portal-control">Start date <input v-model="from" type="date" :max="to" required /></label>
      <label class="portal-control">End date <input v-model="to" type="date" :min="from" required /></label>
      <p class="muted portal-note">{{ spaceOptions.find(option => option.value === space)?.label }} · Raw records for this date range</p>
      <Notice v-if="exportError" tone="error">{{ exportError }}</Notice>
      <div class="modal-actions">
        <button type="button" :disabled="saving" @click="confirmExport = false">Cancel</button>
        <button type="submit" class="primary" :disabled="saving || from > to">{{ saving ? 'Exporting…' : 'Export raw records' }}</button>
      </div>
    </form>
  </Modal>
</template>

<style scoped>
.event-target { font-weight: 400; font-size: 12px; }
</style>
