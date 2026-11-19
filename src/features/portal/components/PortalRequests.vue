<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ArrowLeft, ArrowRight, RefreshCw } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { governanceApi } from '../api/governance'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import { projectSettingNames, quotaBytes, requestLabels, statusOptions, type AdminResponse, type PortalRequest, type QuotaDirection, type QuotaUnit, type RequestType } from '../model/governance'
import type { RequestStatus } from '../model/sharing'
import type { Page, PortalScope, PortalUsage } from '../model/spaces'

const props = defineProps<{ id: string; scope?: PortalScope; manager?: boolean }>()
const emit = defineEmits<{ collaborators: []; settings: [] }>()
const listing = usePortalTask<Page<PortalRequest>>()
const updates = usePortalTask<PortalRequest>()
const storage = usePortalTask<PortalUsage>()
const { data: requests, pending: loading, error: readError } = listing
const { pending, error } = updates
const admin = computed(() => !props.scope)
const page = ref(1)
const search = ref('')
const status = ref<RequestStatus | ''>('')
const type = ref<RequestType | ''>('')
const types = [{ value: '' as const, label: 'All request types' },
  ...(Object.keys(requestLabels) as RequestType[]).map(value => ({ value, label: requestLabels[value] }))]
const selected = ref<PortalRequest>()
const response = ref<AdminResponse['kind']>()
const note = ref('')
const quota = ref(false)
const quotaTarget = ref<number>(0)
const unit = ref<QuotaUnit>('GiB')
const direction = ref<QuotaDirection>('increase')
const units = ['MiB', 'GiB', 'TiB'].map(value => ({ value: value as QuotaUnit, label: value }))
const directions = [{ value: 'increase', label: 'Increase' }, { value: 'decrease', label: 'Decrease' }] as const
const tooSmall = computed(() => !!storage.data.value?.used_bytes && quotaBytes(quotaTarget.value, unit.value) < storage.data.value.used_bytes)
const visible = computed(() => {
  const rows = requests.value?.items ?? []
  if (admin.value) return rows
  const query = search.value.trim().toLowerCase()
  return rows.filter(row => (!status.value || row.status === status.value) && (!type.value || row.request_type === type.value) &&
    (!query || [String(row.id), row.requester_email, describe(row)].some(value => value.toLowerCase().includes(query))))
})

onMounted(() => void refresh())

async function refresh(next = page.value): Promise<void> {
  page.value = next
  const snapshot = await listing.run(async () => {
    if (props.scope) {
      const items = await governanceApi.requests(props.scope)
      return { items, page: 1, page_size: items.length, total: items.length, has_next: false }
    }
    return governanceApi.adminRequests(props.id, { page: next, page_size: 25, search: search.value.trim(),
      status: status.value || undefined, request_type: type.value || undefined })
  })
  const current = selected.value
  if (current) selected.value = snapshot?.items.find(item => item.id === current.id) ?? current
}

function describe(request: PortalRequest): string {
  const payload = request.payload
  if (request.request_type === 'portal_setting_change') {
    const setting = payload.setting ? projectSettingNames[payload.setting] : 'Project policy'
    if (payload.mode === 'inherit') return `${setting} · Inherit policy`
    const value = typeof payload.value === 'boolean' ? payload.value ? 'Enabled' : 'Disabled'
      : Array.isArray(payload.value) ? payload.value.join(', ') || 'None' : (payload.value ?? '—')
    return `${setting} · ${value}`
  }
  if (request.request_type === 'account_quota_change') return `${payload.direction ?? ''} to ${payload.target_quota_value ?? '—'} ${payload.target_quota_unit ?? ''}`
  return [payload.target_name || payload.target_email, payload.portal_role?.replace('portal_', 'Portal ')].filter(Boolean).join(' · ') || requestLabels[request.request_type]
}

function inspect(request: PortalRequest): void {
  updates.clear()
  selected.value = request
  response.value = undefined
}

function openQuota(): void {
  if (!props.scope) return
  const scope = props.scope
  updates.clear()
  quotaTarget.value = 0
  note.value = ''
  quota.value = true
  void storage.run(() => portalApi.usage(scope))
}

async function requestQuota(): Promise<void> {
  const scope = props.scope
  if (!scope || tooSmall.value || pending.value) return
  const updated = await updates.run(() => governanceApi.requestQuota(scope, direction.value, quotaTarget.value, unit.value, note.value.trim() || null))
  if (updated) {
    quota.value = false
    await refresh()
    inspect(updated)
  }
}

function startResponse(kind: AdminResponse['kind']): void {
  updates.clear()
  response.value = kind
  note.value = ''
}

async function submitResponse(): Promise<void> {
  const current = selected.value
  const kind = response.value
  if (!current || !kind || pending.value || !admin.value) return
  const text = note.value.trim()
  if (kind === 'message' && !text || kind !== 'message' && current.status !== 'pending') return
  const decision = kind === 'message' ? { kind, message: text } : { kind, message: text || null }
  const updated = await updates.run(() => governanceApi.respond(props.id, current.id, decision))
  if (updated) {
    selected.value = updated
    response.value = undefined
  }
  await refresh()
  if (selected.value?.status !== 'pending' && kind !== 'message') response.value = undefined
}
</script>

<template>
  <section class="portal-panel">
    <div class="portal-between">
      <h2>{{ admin ? 'Request triage' : 'Project requests' }}</h2>
      <div v-if="!admin && manager" class="portal-inline">
        <button type="button" @click="emit('collaborators')">Manage collaborators</button>
        <button type="button" @click="emit('settings')">Project settings</button>
        <button type="button" class="primary" @click="openQuota">Request quota change</button>
      </div>
    </div>
    <form class="portal-inline" @submit.prevent="refresh(1)">
      <input v-model="search" type="search" aria-label="Search requests" placeholder="Search requests" />
      <AppSelect v-model="status" :options="statusOptions" aria-label="Request status" @update:model-value="admin && refresh(1)" />
      <AppSelect v-model="type" :options="types" aria-label="Request type" @update:model-value="admin && refresh(1)" />
      <button type="submit" :disabled="loading"><RefreshCw :size="14" /> Refresh</button>
    </form>
    <Notice v-if="readError" tone="error">{{ readError }}</Notice>
    <p v-if="loading" class="muted portal-note" role="status">Loading requests…</p>
    <div v-if="requests" class="portal-table-scroll">
      <table class="portal-table">
        <thead><tr><th scope="col">Request</th><th scope="col">Requester</th><th scope="col">State</th><th scope="col">Updated</th></tr></thead>
        <tbody>
          <tr v-for="item in visible" :key="item.id">
            <th scope="row">
              <button type="button" class="subtle portal-button-label" @click="inspect(item)">#{{ item.id }} {{ requestLabels[item.request_type] }}</button>
              <div class="muted request-meta">{{ describe(item) }}</div>
            </th>
            <td>{{ item.requester_email }}<div v-if="admin" class="muted request-meta">{{ item.account_name ?? item.account_id }}</div></td>
            <td><span class="chip">{{ item.status }}</span></td><td>{{ formatDate(item.updated_at, true) }}</td>
          </tr>
          <tr v-if="!visible.length"><td colspan="4" class="muted">No matching requests.</td></tr>
        </tbody>
      </table>
    </div>
    <div v-if="admin && (page > 1 || requests?.has_next)" class="portal-inline">
      <button type="button" :disabled="loading || page <= 1" @click="refresh(page - 1)"><ArrowLeft :size="14" /> Previous</button>
      <span>Page {{ page }}</span>
      <button type="button" :disabled="loading || !requests?.has_next" @click="refresh(page + 1)">Next <ArrowRight :size="14" /></button>
    </div>
  </section>
  <Modal v-if="selected" :title="`Request #${selected.id} · ${requestLabels[selected.request_type]}`" drawer @close="!pending && (selected = undefined)">
    <div class="modal-form">
      <div class="portal-between">
        <strong>{{ describe(selected) }}</strong><span class="chip">{{ selected.status }}</span>
      </div>
      <dl class="setup-summary">
        <div><dt>Project</dt><dd>{{ selected.account_name ?? selected.account_id }}</dd></div>
        <div><dt>Requester</dt><dd>{{ selected.requester_email }}</dd></div>
        <div><dt>Submitted</dt><dd>{{ formatDate(selected.created_at, true) }}</dd></div>
        <div v-if="selected.decided_by_email"><dt>Decision</dt><dd>{{ selected.decided_by_email }} · {{ formatDate(selected.decided_at, true) }}</dd></div>
      </dl>
      <p v-if="selected.payload.reason" class="portal-note">{{ selected.payload.reason }}</p>
      <Notice v-if="selected.error_message" tone="error">{{ selected.error_message }}</Notice>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <section class="portal-panel">
        <h2>Messages</h2>
        <ul v-if="selected.messages?.length" class="portal-list">
          <li v-for="message in selected.messages" :key="message.id">
            <div class="portal-between"><strong>{{ message.author_email }}</strong><span class="muted">{{ formatDate(message.created_at, true) }}</span></div>
            <p class="portal-note">{{ message.message }}</p>
          </li>
        </ul>
        <p v-else class="muted portal-note">No messages.</p>
      </section>
      <div v-if="admin && !response" class="portal-inline">
        <button v-if="selected.status === 'pending'" type="button" class="primary" @click="startResponse('approve')">Approve</button>
        <button v-if="selected.status === 'pending'" type="button" @click="startResponse('reject')">Reject</button>
        <button type="button" @click="startResponse('message')">Add message</button>
      </div>
      <form v-if="response" class="modal-form" @submit.prevent="submitResponse">
        <label class="portal-control">{{ response === 'message' ? 'Message to requester' : 'Decision message' }}
          <textarea v-model="note" maxlength="2000" :required="response === 'message'" :disabled="pending" />
        </label>
        <div class="portal-inline">
          <button type="button" :disabled="pending" @click="response = undefined">Cancel</button>
          <button type="submit" class="primary" :disabled="pending">{{ pending ? 'Waiting for the service…' : response === 'message' ? 'Send message' : `Confirm ${response === 'approve' ? 'approval' : 'rejection'}` }}</button>
        </div>
      </form>
    </div>
  </Modal>
  <Modal v-if="quota" title="Request a quota change" @close="!pending && (quota = false)">
    <form class="modal-form" @submit.prevent="requestQuota">
      <p class="muted portal-note">Current use {{ formatSize(storage.data.value?.used_bytes) }} · Quota {{ formatSize(storage.data.value?.quota_max_size_bytes) }}</p>
      <AppSelect v-model="direction" :options="directions" aria-label="Quota direction" />
      <div class="portal-inline">
        <label class="portal-control">Target quota <input v-model.number="quotaTarget" type="number" min="0.001" step="any" required /></label>
        <AppSelect v-model="unit" :options="units" aria-label="Quota unit" />
      </div>
      <label class="portal-control">Reason <textarea v-model="note" maxlength="2000" /></label>
      <Notice v-if="tooSmall" tone="error">The target quota is below measured project usage.</Notice>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <footer class="modal-actions">
        <button type="button" :disabled="pending" @click="quota = false">Cancel</button>
        <button type="submit" class="primary" :disabled="pending || tooSmall || quotaTarget <= 0">Submit quota request</button>
      </footer>
    </form>
  </Modal>
</template>

<style scoped>
.request-meta { margin-top: 3px; font-weight: 400; font-size: 12px; }
</style>
