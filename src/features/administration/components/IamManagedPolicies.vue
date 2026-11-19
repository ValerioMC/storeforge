<script setup lang="ts">
import { computed, onScopeDispose, ref } from 'vue'
import { ArrowLeft, Plus } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { Provider } from '../../connection/model/connection'
import { writeQuery } from '../api/iam'
import { createManagedPolicy, inspectManagedPolicy, listManagedPolicies, listPolicyVersions, readPolicyVersion } from '../api/resources'
import { useQueryPage } from '../composables/useQueryPage'
import { validPolicy, type QueryPage } from '../model/iam'
import { knownCapabilityError, type ManagedPolicy, type PolicyVersion } from '../model/resources'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ connection: string; provider: Provider }>()
const selected = ref<ManagedPolicy>()
const editing = ref<{ name: string; description: string; document: string }>()
const newVersion = ref(false)
const makeDefault = ref(false)
const versionDocument = ref('')
const viewedVersion = ref<{ version: string; document: string }>()
const pending = ref<{ action: 'SetDefaultPolicyVersion' | 'DeletePolicyVersion' | 'DeletePolicy'; label: string; version?: string }>()
const error = ref('')
const busy = ref(false)
const unsupportedCreate = ref(false)
const canCreate = computed(() => props.provider !== 'ceph' && !unsupportedCreate.value)
const canEdit = computed(() => !!selected.value && canCreate.value && selected.value.arn.split(':')[4] !== 'aws')
const page = useQueryPage(() => props.connection, marker => listManagedPolicies(props.connection, marker), policy => `${policy.name} ${policy.arn}`)
const versions = useQueryPage(() => selected.value?.arn, readVersions, version => version.id)
let generation = 0
onScopeDispose(() => { generation++ })

function readVersions(marker?: string): Promise<QueryPage<PolicyVersion>> {
  return selected.value ? listPolicyVersions(props.connection, selected.value.arn, marker) : Promise.resolve({ items: [], marker: null })
}

async function open(policy: ManagedPolicy): Promise<void> {
  const current = ++generation
  selected.value = policy
  error.value = ''
  busy.value = true
  try {
    const result = await inspectManagedPolicy(props.connection, policy.arn)
    if (current === generation) selected.value = result
  } catch (failure) {
    if (current === generation) error.value = errorMessage(failure)
  } finally {
    if (current === generation) busy.value = false
  }
}

async function inspect(version: PolicyVersion): Promise<void> {
  if (!selected.value || busy.value) return
  const current = generation
  error.value = ''
  busy.value = true
  try {
    const document = await readPolicyVersion(props.connection, selected.value.arn, version.id)
    if (current === generation) viewedVersion.value = { version: version.id, document }
  } catch (failure) {
    if (current === generation) error.value = errorMessage(failure)
  } finally {
    if (current === generation) busy.value = false
  }
}

async function create(): Promise<void> {
  if (!editing.value || !canCreate.value || busy.value) return
  const current = ++generation
  const draft = editing.value
  error.value = ''
  busy.value = true
  try {
    const policy = await createManagedPolicy(props.connection, draft.name.trim(), draft.document, draft.description)
    if (current === generation) {
      editing.value = undefined
      await open(policy)
      await page.reload()
    }
  } catch (failure) {
    if (current === generation) {
      unsupportedCreate.value = knownCapabilityError(failure)
      if (unsupportedCreate.value) editing.value = undefined
      else error.value = errorMessage(failure)
    }
  } finally {
    if (current === generation) busy.value = false
  }
}

async function mutate(action: string, changes: Record<string, string> = {}): Promise<void> {
  const policy = selected.value
  if (!policy || !canEdit.value || busy.value) return
  const current = ++generation
  pending.value = undefined
  error.value = ''
  busy.value = true
  try {
    await writeQuery(props.connection, action, { PolicyArn: policy.arn, ...changes })
    if (current !== generation) return
    if (action === 'DeletePolicy') {
      selected.value = undefined
      await page.reload()
    } else if (action === 'CreatePolicyVersion') {
      newVersion.value = false
    }
  } catch (failure) {
    if (current === generation) error.value = knownCapabilityError(failure) ? 'This endpoint does not support changing managed policy versions.' : errorMessage(failure)
  }
  if (current === generation) {
    const issue = error.value
    if (selected.value) {
      await open(policy)
      error.value = [issue, error.value].filter(Boolean).join('\n')
      await versions.reload()
    }
    busy.value = false
  }
}

function confirm(): void {
  const change = pending.value
  if (change) void mutate(change.action, change.version ? { VersionId: change.version } : {})
}
</script>

<template>
  <div class="panel-form">
    <template v-if="!selected">
      <div class="stage-heading">
        <h2>Managed IAM policies</h2>
        <button v-if="canCreate" class="primary" @click="editing = { name: '', description: '', document: '' }; error = ''"><Plus :size="16" /> Create policy</button>
      </div>
      <Notice v-if="!canCreate">{{ provider === 'ceph' ? 'Ceph RGW provides a fixed set of managed policies. Use inline policies for custom permissions.' : 'This IAM endpoint does not support managed policy creation.' }}</Notice>
      <form class="form-grid" @submit.prevent="page.reload">
        <label>Search policies<input v-model="page.search.value" type="search" /></label>
        <div class="panel-actions"><button :disabled="page.loading.value">Search</button><button type="button" :disabled="page.loading.value" @click="page.reload">Refresh</button></div>
      </form>
      <Notice v-if="page.error.value" tone="error">{{ page.error.value }}</Notice>
      <p v-else-if="page.loading.value" role="status">Loading policies…</p>
      <p v-else-if="!page.items.value.length" class="muted">No matching policies.</p>
      <div v-for="policy in page.items.value" :key="policy.arn" class="settings-row">
        <div class="settings-description"><strong>{{ policy.name }}</strong><p>{{ policy.arn }}</p></div>
        <button :aria-label="`Open ${policy.name}`" @click="open(policy)">Open</button>
      </div>
      <QueryPager :busy="page.loading.value" :previous="!!page.markers.value.length" :next="!!page.nextMarker.value" @previous="page.previous" @next="page.next" />
    </template>
    <template v-else>
      <button :disabled="busy" @click="selected = undefined; error = ''"><ArrowLeft :size="15" /> Managed policies</button>
      <div class="stage-heading">
        <div><h2>{{ selected.name }}</h2><p>{{ selected.arn }}</p><p v-if="selected.description">{{ selected.description }}</p></div>
        <button v-if="canEdit" :disabled="busy" @click="pending = { action: 'DeletePolicy', label: 'Delete policy' }">Delete policy</button>
      </div>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <div class="stage-heading">
        <h3>Policy versions</h3>
        <button v-if="canEdit" :disabled="busy" @click="newVersion = true; versionDocument = ''; makeDefault = false">Create version</button>
      </div>
      <p v-if="!canEdit" class="muted">Provider-managed policies are read-only.</p>
      <Notice v-if="versions.error.value" tone="error">{{ versions.error.value }}</Notice>
      <p v-else-if="versions.loading.value" role="status">Loading versions…</p>
      <p v-else-if="!versions.items.value.length" class="muted">The provider returned no version history.</p>
      <div v-for="version in versions.items.value" :key="version.id" class="settings-row">
        <div class="settings-description"><strong>{{ version.id }}</strong><p>{{ version.created }}</p></div>
        <span v-if="version.default" class="chip">Default</span>
        <button :disabled="busy" @click="inspect(version)">View JSON</button>
        <template v-if="canEdit && !version.default">
          <button :disabled="busy" @click="pending = { action: 'SetDefaultPolicyVersion', label: 'Set as default', version: version.id }">Set as default</button>
          <button :disabled="busy" @click="pending = { action: 'DeletePolicyVersion', label: 'Delete version', version: version.id }">Delete</button>
        </template>
      </div>
      <QueryPager :busy="versions.loading.value" :previous="!!versions.markers.value.length" :next="!!versions.nextMarker.value" @previous="versions.previous" @next="versions.next" />
    </template>
    <Modal v-if="editing" title="Create managed policy" wide @close="!busy && (editing = undefined)">
      <form class="modal-form" @submit.prevent="create">
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <label>Policy name<input v-model="editing.name" required :disabled="busy" /></label>
        <label>Description<input v-model="editing.description" :disabled="busy" /></label>
        <label>Policy JSON<textarea v-model="editing.document" rows="10" spellcheck="false" required :disabled="busy" /></label>
        <div class="modal-actions"><button type="button" @click="editing = undefined" :disabled="busy">Cancel</button>
          <button class="primary" :disabled="busy || !editing.name.trim() || !validPolicy(editing.document)">Create policy</button></div>
      </form>
    </Modal>
    <Modal v-if="newVersion" title="Create policy version" wide @close="!busy && (newVersion = false)">
      <form class="modal-form" @submit.prevent="mutate('CreatePolicyVersion', { PolicyDocument: versionDocument, SetAsDefault: String(makeDefault) })">
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <label>Policy JSON<textarea v-model="versionDocument" rows="10" spellcheck="false" required :disabled="busy" /></label>
        <label class="check-label"><input v-model="makeDefault" type="checkbox" :disabled="busy" /> Set this version as default</label>
        <div class="modal-actions"><button type="button" @click="newVersion = false" :disabled="busy">Cancel</button>
          <button class="primary" :disabled="busy || !validPolicy(versionDocument)">Create version</button></div>
      </form>
    </Modal>
    <Modal v-if="viewedVersion" :title="`Policy version ${viewedVersion.version}`" wide @close="viewedVersion = undefined">
      <div class="modal-form"><label>Policy JSON<textarea :value="viewedVersion.document" rows="15" readonly /></label></div>
    </Modal>
    <ConfirmDialog v-if="pending && selected" :title="`${pending.label}?`"
      :description="pending.version ? `${selected.name} · ${pending.version}` : selected.name"
      :action="pending.label" :danger="pending.action.startsWith('Delete')" @close="pending = undefined" @confirm="confirm" />
  </div>
</template>

<style scoped>
.settings-description p { overflow-wrap: anywhere; }
</style>
