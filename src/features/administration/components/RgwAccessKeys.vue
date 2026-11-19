<script setup lang="ts">
import { computed, onScopeDispose, ref } from 'vue'
import { Plus, RefreshCw } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { useConnections } from '../../connection/store/connections'
import type { Connection } from '../../connection/model/connection'
import { changeRgwIdentity } from '../api/rgw'
import { rgwUserLabel, type IssuedRgwKey, type RgwKey, type RgwKeyChange, type RgwKeyScope, type RgwUser } from '../model/rgw'

const props = defineProps<{ connection: string; owner: RgwUser; scope: RgwKeyScope; loading: boolean; reload: () => Promise<void> }>()
const emit = defineEmits<{ open: [profile: Connection] }>()
const connections = useConnections()
const editing = ref<{ key?: RgwKey; private: boolean }>()
const pending = ref<{ change: RgwKeyChange; label: string }>()
const issued = ref<IssuedRgwKey>()
const provisioned = ref<Connection>()
const name = ref('')
const notes = ref('')
const expiration = ref('')
const error = ref('')
const issue = ref('')
const busy = ref(false)
const ownerLabel = computed(() => rgwUserLabel(props.owner.identity))
const keyActionsDisabled = computed(() => busy.value || props.loading)
let disposed = false
onScopeDispose(() => {
  disposed = true
  issued.value = undefined
})

function edit(key?: RgwKey, makePrivate = false): void {
  editing.value = { key, private: makePrivate }
  error.value = ''
  name.value = key?.metadata.name ?? ''
  notes.value = key?.metadata.notes ?? ''
  const value = key?.metadata.expiresAt
  if (!value) {
    expiration.value = ''
    return
  }
  const instant = new Date(value)
  const local = new Date(instant.getTime() - instant.getTimezoneOffset() * 60000)
  expiration.value = local.toISOString().slice(0, 16)
}

function reviewMetadata(): void {
  const draft = editing.value
  if (!draft || busy.value) return
  const entered = expiration.value ? new Date(expiration.value) : null
  if (entered && !Number.isFinite(entered.getTime())) {
    error.value = 'Enter a valid expiration date.'
    return
  }
  const existing = draft.key?.metadata.expiresAt ?? null
  const expiresAt = entered && existing && Math.floor(entered.getTime() / 60000) === Math.floor(Date.parse(existing) / 60000)
    ? existing : entered?.toISOString() ?? null
  const metadata = { name: name.value, notes: notes.value, expiresAt }
  const change: RgwKeyChange = draft.key
    ? { action: 'metadata', id: draft.key.id, metadata }
    : { action: 'create', metadata, private: draft.private }
  void apply(change)
}

function reviewStatus(key: RgwKey): void {
  const active = key.active !== true
  pending.value = { change: { action: 'status', id: key.id, active }, label: active ? 'Activate key' : 'Deactivate key' }
}

function reviewDelete(key: RgwKey): void {
  pending.value = { change: { action: 'delete', id: key.id }, label: key.providerMissing ? 'Remove local profile' : 'Delete key' }
}

function confirm(): void {
  if (pending.value) void apply(pending.value.change)
}

async function apply(change: RgwKeyChange): Promise<void> {
  if (busy.value) return
  pending.value = undefined
  busy.value = true
  error.value = ''
  issue.value = ''
  try {
    const result = await changeRgwIdentity(props.connection, { action: 'key', scope: props.scope, owner: props.owner.identity, change })
    if (disposed) return
    editing.value = undefined
    issued.value = result.issued ?? undefined
    provisioned.value = result.profile ?? undefined
    issue.value = result.issue ?? ''
    if (result.profile || change.action === 'delete') await connections.refreshProfile(props.connection)
  } catch (failure) {
    if (!disposed) error.value = errorMessage(failure)
  } finally {
    if (!disposed) await props.reload()
    busy.value = false
  }
}
</script>

<template>
  <section class="panel-form">
    <div class="stage-heading">
      <div><h3>{{ scope === 'delegated' ? 'Delegated RGW keys' : 'RGW access keys' }}</h3><p>{{ ownerLabel }}{{ owner.account ? ` · ${owner.account}` : '' }}</p></div>
      <div class="panel-actions">
        <button :disabled="keyActionsDisabled" @click="reload"><RefreshCw :size="15" /> Refresh keys</button>
        <button v-if="owner.keys" :disabled="keyActionsDisabled || owner.suspended === true" @click="edit(undefined, true)">Provision private connection</button>
        <button v-if="owner.keys" class="primary" :disabled="keyActionsDisabled || owner.suspended === true" @click="edit()"><Plus :size="15" /> Create RGW key</button>
      </div>
    </div>
    <Notice v-if="error && !editing" tone="error">{{ error }}</Notice>
    <Notice v-if="issue" tone="error">{{ issue }}</Notice>
    <Notice v-if="provisioned">
      Private connection {{ provisioned.name }} is ready.
      <button type="button" @click="emit('open', provisioned)">Open connection</button>
    </Notice>
    <p v-if="loading" role="status">Refreshing RGW keys…</p>
    <p v-else-if="owner.keys === null" class="muted">RGW did not report key inventory for this user.</p>
    <p v-else-if="!owner.keys.length" class="muted">No RGW access keys.</p>
    <div v-for="key in owner.keys ?? []" :key="key.id" class="settings-row rgw-key-row">
      <div class="settings-description">
        <strong>{{ key.metadata.name || key.id }}</strong>
        <p v-if="key.metadata.name">{{ key.id }}</p>
        <p v-if="key.metadata.notes">{{ key.metadata.notes }}</p>
        <p v-if="key.metadata.expiresAt">Expires {{ new Date(key.metadata.expiresAt).toLocaleString() }} · Enforced by Storeforge desktop while running</p>
        <p v-if="key.enforcement?.state === 'disabled'">Expiration enforced</p>
        <Notice v-if="key.enforcement?.state === 'failed'" tone="error">Expiration failed. {{ key.enforcement.message }}</Notice>
        <Notice v-if="key.issue" tone="error">{{ key.issue }}</Notice>
      </div>
      <span class="chip">{{ key.providerMissing ? 'Missing from RGW' : key.active == null ? 'State unavailable' : key.active ? 'Active' : 'Inactive' }}</span>
      <span v-if="key.privateProfile" class="chip">Private connection</span>
      <div class="panel-actions">
        <button v-if="!key.providerMissing" :disabled="keyActionsDisabled" @click="edit(key)">Edit metadata</button>
        <button v-if="key.active != null && key.id !== owner.connectionKeyId" :disabled="keyActionsDisabled" @click="reviewStatus(key)">{{ key.active ? 'Deactivate' : 'Activate' }}</button>
        <button v-if="key.id !== owner.connectionKeyId" :disabled="keyActionsDisabled" @click="reviewDelete(key)">{{ key.providerMissing ? 'Remove local profile' : 'Delete' }}</button>
        <span v-else class="muted">Connection credential</span>
      </div>
    </div>
    <Modal v-if="editing" :title="editing.key ? 'Edit RGW key metadata' : editing.private ? 'Provision private connection' : 'Create RGW key'" @close="!busy && (editing = undefined)">
      <form class="modal-form" @submit.prevent="reviewMetadata">
        <p>Owner · {{ ownerLabel }}</p>
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <label>Name<input v-model="name" maxlength="150" :required="editing.private" :disabled="busy" /></label>
        <label>Notes<textarea v-model="notes" rows="3" maxlength="2000" :disabled="busy" /></label>
        <template v-if="editing.key?.id !== owner.connectionKeyId || !editing.key">
          <label>Expiration (optional)<input v-model="expiration" type="datetime-local" :disabled="busy" /></label>
          <p class="muted">Storeforge desktop checks for expired keys while running. It retries failed deactivations and reports the result here.</p>
        </template>
        <div class="modal-actions">
          <button type="button" :disabled="busy" @click="editing = undefined">Cancel</button>
          <button class="primary" :disabled="busy">{{ editing.key ? 'Save metadata' : editing.private ? 'Provision connection' : 'Issue key' }}</button>
        </div>
      </form>
    </Modal>
    <Modal v-if="issued" title="New RGW access key" @close="issued = undefined">
      <div class="modal-form">
        <p>Copy the secret before closing this dialog.</p>
        <Notice v-if="issue" tone="error">{{ issue }}</Notice>
        <label>Owner<input :value="ownerLabel" readonly /></label>
        <label>Access key ID<input :value="issued.accessKey" readonly /></label>
        <label>Secret access key<input :value="issued.secretKey" readonly autocomplete="off" spellcheck="false" /></label>
        <div class="modal-actions"><button class="primary" @click="issued = undefined">Done</button></div>
      </div>
    </Modal>
    <ConfirmDialog v-if="pending" :title="`${pending.label}?`"
      :description="`This changes ${'id' in pending.change ? pending.change.id : ''} for ${ownerLabel} on the selected RGW endpoint.`"
      :action="pending.label" :danger="pending.change.action === 'delete'" @close="pending = undefined" @confirm="confirm" />
  </section>
</template>

<style scoped>
.rgw-key-row { flex-wrap: wrap; }
.settings-description { overflow-wrap: anywhere; }
</style>
