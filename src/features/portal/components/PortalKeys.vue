<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Copy, Download, KeyRound } from 'lucide-vue-next'
import { formatDate } from '../../../common/format'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { portalApi } from '../api/portal'
import { sharingApi } from '../api/sharing'
import { usePortalTask } from '../composables/usePortalTask'
import { clientConnection, clientDocument, type ClientExport } from '../model/client-export'
import type { KeyPermission, KeyReceipt, PortalKey, PortalKeys } from '../model/sharing'
import type { PortalScope, PortalSpace } from '../model/spaces'

const props = defineProps<{ scope: PortalScope }>()
const keyList = usePortalTask<PortalKeys>()
const spaces = usePortalTask<PortalSpace[]>()
const mutation = usePortalTask<boolean>()
const { data: keys } = keyList
const { error, pending, run } = mutation
const receipt = ref<KeyReceipt>()
const target = ref<PortalKey>()
const action = ref<'personal' | 'external' | 'status' | 'delete' | 'setup'>()
const selectedSpace = ref('')
const recipient = ref('')
const permission = ref<KeyPermission>('read_only')
const message = ref('')
const permissions = [{ value: 'read_only', label: 'Read only' }, { value: 'read_write', label: 'Read and write' }] as const
const formats: { value: ClientExport; label: string }[] = [
  { value: 'cyberduck', label: 'Cyberduck / Mountain Duck' }, { value: 'winscp', label: 'WinSCP' }, { value: 'generic', label: 'Generic S3 settings' },
]
const selection = computed(() => spaces.data.value?.find(space => space.id === selectedSpace.value))
const scopeChoices = computed(() => (spaces.data.value ?? []).filter(space => space.can_browse && space.status !== 'Archived'))
const externalChoices = computed(() => scopeChoices.value.filter(space => space.role === 'Owner' || space.role === 'Manager'))
const scopeOptions = computed(() => scopeChoices.value.map(space => ({ value: space.id, label: space.name })))
const externalOptions = computed(() => externalChoices.value.map(space => ({ value: space.id, label: space.name })))
const personalCount = computed(() => keys.value?.access_keys.filter(key => key.target_type === 'self').length ?? 0)
const connection = computed(() => {
  const snapshot = keys.value
  const key = target.value
  if (!snapshot || !key) return undefined
  const bucket = key.target_type === 'external' ? key.bucket_name : selection.value?.internal_bucket_name || null
  return clientConnection(snapshot.s3_endpoint, { access_key_id: key.access_key_id, bucket_name: bucket }, snapshot.force_path_style, selection.value?.region ?? null)
})

onMounted(() => {
  void refresh()
  void spaces.run(() => portalApi.spaces(props.scope))
})

async function refresh(): Promise<void> {
  await keyList.run(() => sharingApi.keys(props.scope))
}

function openAction(next: typeof action.value, key?: PortalKey): void {
  mutation.clear()
  recipient.value = ''
  permission.value = 'read_only'
  message.value = ''
  target.value = key
  selectedSpace.value = (next === 'external' ? externalOptions.value[0] : scopeOptions.value[0])?.value ?? ''
  action.value = next
}

async function submit(): Promise<void> {
  const requested = action.value
  const key = target.value
  if (!requested || pending.value) return
  const done = await run(async () => {
    if (requested === 'personal') receipt.value = await sharingApi.personalKey(props.scope)
    if (requested === 'external') receipt.value = await sharingApi.externalKey(props.scope, selectedSpace.value, recipient.value.trim(), permission.value)
    if (requested === 'status' && key) await sharingApi.keyStatus(props.scope, key.access_key_id, !key.is_active)
    if (requested === 'delete' && key) await sharingApi.deleteKey(props.scope, key.access_key_id)
    return true
  })
  if (done) {
    action.value = undefined
    message.value = requested === 'delete' ? 'Access key deleted.' : requested === 'status' ? 'Access key status updated.' : 'Access key created.'
    await refresh()
  }
}

async function copySecret(): Promise<void> {
  if (!receipt.value?.secret_access_key) return
  const secret = receipt.value.secret_access_key
  await run(async () => {
    await navigator.clipboard.writeText(secret)
    message.value = 'Secret copied.'
    return true
  })
}

async function exportSettings(format: ClientExport): Promise<void> {
  const settings = connection.value
  if (!settings) return
  await run(async () => {
    const saved = await sharingApi.exportClient(format, clientDocument(format, settings))
    if (saved) message.value = 'Client settings saved.'
    return saved
  })
}
</script>

<template>
  <section class="portal-panel">
    <div class="portal-between">
      <h2><KeyRound :size="16" /> S3 access keys</h2>
      <div v-if="keys" class="portal-inline">
        <button v-if="keys.can_create_external_access" type="button" :disabled="!externalOptions.length" @click="openAction('external')">Create external key</button>
        <button v-if="keys.can_manage_access_keys" type="button" class="primary" :disabled="personalCount >= keys.max_access_keys" @click="openAction('personal')">Create personal key</button>
      </div>
    </div>
    <Notice v-if="keyList.error.value" tone="error">{{ keyList.error.value }}</Notice>
    <Notice v-if="spaces.error.value" tone="error">{{ spaces.error.value }}</Notice>
    <Notice v-if="message && !receipt && !action" tone="success">{{ message }}</Notice>
    <p v-if="keyList.pending.value && !keys" class="muted portal-note" role="status">Loading access keys…</p>
    <template v-if="keys">
      <p class="muted portal-note">{{ personalCount }} / {{ keys.max_access_keys }} personal keys</p>
      <div v-if="keys.access_keys.length" class="portal-table-scroll">
        <table class="portal-table">
          <thead><tr><th scope="col">Access ID</th><th scope="col">Scope</th><th scope="col">Status</th><th scope="col">Actions</th></tr></thead>
          <tbody>
            <tr v-for="key in keys.access_keys" :key="key.access_key_id">
              <th scope="row"><code>{{ key.access_key_id }}</code><div v-if="key.external_email" class="muted key-meta">{{ key.external_email }}</div></th>
              <td>{{ key.storage_space_name || 'Personal' }}<div v-if="key.permission" class="muted key-meta">{{ key.permission === 'read_only' ? 'Read only' : 'Read and write' }}</div></td>
              <td>{{ key.status ?? (key.is_active ? 'Active' : 'Disabled') }}<div v-if="key.expires_at" class="muted key-meta">{{ formatDate(key.expires_at, true) }}</div></td>
              <td><div class="portal-inline">
                <button v-if="key.is_active" type="button" @click="openAction('setup', key)">Client setup</button>
                <template v-if="key.deletable">
                  <button v-if="key.status !== 'RevocationPending'" type="button" @click="openAction('status', key)">{{ key.is_active ? 'Disable' : 'Enable' }}</button>
                  <button type="button" @click="openAction('delete', key)">Delete</button>
                </template>
              </div></td>
            </tr>
          </tbody>
        </table>
      </div>
      <p v-else class="muted portal-note">No access keys for this project membership.</p>
    </template>
  </section>
  <Modal v-if="receipt" title="Access key receipt" @close="!pending && (receipt = undefined)">
    <div class="modal-form">
      <p class="portal-note">Copy the secret now. The service only returns it when a key is issued.</p>
      <label class="portal-control">Access ID <input :value="receipt.access_key_id" readonly /></label>
      <label v-if="receipt.secret_access_key" class="portal-control">Secret <input :value="receipt.secret_access_key" readonly autocomplete="off" /></label>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="message === 'Secret copied.'" tone="success">{{ message }}</Notice>
      <div class="modal-actions">
        <button v-if="receipt.secret_access_key" type="button" :disabled="pending" @click="copySecret"><Copy :size="15" /> Copy secret</button>
        <button type="button" class="primary" :disabled="pending" @click="receipt = undefined">Done</button>
      </div>
    </div>
  </Modal>
  <Modal v-if="action && !receipt" :title="{ personal: 'Create a personal key', external: 'Create a space-scoped key', status: target?.is_active ? 'Disable access key' : 'Enable access key', delete: 'Delete access key', setup: 'External client setup' }[action]"
    @close="!pending && (action = undefined)">
    <form class="modal-form" @submit.prevent="submit">
      <p v-if="action === 'personal'" class="portal-note">This key uses your current project storage permissions.</p>
      <template v-else-if="action === 'external'">
        <AppSelect v-model="selectedSpace" :options="externalOptions" aria-label="Storage space" required />
        <label class="portal-control">Recipient label <input v-model="recipient" required maxlength="254" /></label>
        <AppSelect v-model="permission" :options="permissions" aria-label="External key permission" />
      </template>
      <template v-else-if="action === 'setup'">
        <AppSelect v-if="target?.target_type === 'self'" v-model="selectedSpace" :options="scopeOptions" aria-label="Client storage space" />
        <template v-if="connection">
          <dl class="setup-summary">
            <div><dt>Endpoint</dt><dd>{{ connection.endpoint.href }}</dd></div>
            <div><dt>Bucket</dt><dd>{{ connection.bucket }}</dd></div>
            <div><dt>Access ID</dt><dd>{{ connection.accessId }}</dd></div>
            <div><dt>Addressing</dt><dd>{{ connection.forcePathStyle ? 'Path style' : 'Virtual hosted' }}</dd></div>
          </dl>
          <p class="muted portal-note">Enter the issued secret when the S3 client prompts for a password.</p>
          <div class="portal-inline">
            <button v-for="format in formats" :key="format.value" type="button" :disabled="pending" @click="exportSettings(format.value)"><Download :size="15" /> {{ format.label }}</button>
          </div>
        </template>
        <Notice v-else>Select a storage space with a published endpoint to export client settings.</Notice>
      </template>
      <p v-else class="portal-note">Key {{ target?.access_key_id }}<span v-if="action === 'delete'"> will be removed permanently.</span></p>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="action === 'setup' && message" tone="success">{{ message }}</Notice>
      <footer class="modal-actions">
        <button type="button" :disabled="pending" @click="action = undefined">{{ action === 'setup' ? 'Close' : 'Cancel' }}</button>
        <button v-if="action !== 'setup'" type="submit" :class="action === 'delete' ? 'danger' : 'primary'" :disabled="pending || action === 'external' && (!selectedSpace || !recipient.trim())">{{ pending ? 'Saving…' : 'Confirm' }}</button>
      </footer>
    </form>
  </Modal>
</template>

<style scoped>
h2 svg { vertical-align: -3px; margin-right: 3px; }
.key-meta { font-size: 12px; font-weight: normal; }
</style>
