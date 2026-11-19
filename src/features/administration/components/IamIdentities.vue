<script setup lang="ts">
import { computed, onScopeDispose, ref } from 'vue'
import { ArrowLeft, Plus, RefreshCw } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import Modal from '../../../components/ui/Modal.vue'
import { createIdentity, inspectIdentity, listIdentities, writeQuery } from '../api/iam'
import { useQueryPage } from '../composables/useQueryPage'
import { managedUser, type IamIdentity, type IdentityKind, type PrivateAccessDraft } from '../model/iam'
import IamIdentityEditor from './IamIdentityEditor.vue'
import IamKeys from './IamKeys.vue'
import IamMembership from './IamMembership.vue'
import IamPolicies from './IamPolicies.vue'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ connection: string; kind: IdentityKind }>()
const emit = defineEmits<{ 'personal-access': [] }>()
const selected = ref<IamIdentity>()
const name = ref('')
const path = ref('')
const creating = ref(false)
const confirming = ref(false)
const editing = ref(false)
const busy = ref(false)
const error = ref('')
const partial = ref<string[]>([])
const tab = ref<'general' | 'membership' | 'keys' | 'policies'>('general')
const readOnly = computed(() => props.kind === 'User' && !!selected.value && managedUser(selected.value.name))
const tabs = computed(() => [
  { value: 'general' as const, label: 'General' },
  { value: 'membership' as const, label: props.kind === 'User' ? 'Groups' : 'Members' },
  ...(props.kind === 'User' ? [{ value: 'keys' as const, label: 'Access keys' }] : []),
  { value: 'policies' as const, label: 'Policies' },
])
const { items, markers, nextMarker, loading, error: listError, search, next, previous, reload } = useQueryPage(
  () => [props.connection, props.kind], marker => listIdentities(props.connection, props.kind, marker),
  item => `${item.name} ${item.path} ${item.arn}`,
)
let generation = 0
onScopeDispose(() => { generation++ })

function display(identity: IamIdentity): void {
  selected.value = identity
  name.value = identity.name
  path.value = identity.path
}

async function open(identity: IamIdentity): Promise<void> {
  const current = ++generation
  display(identity)
  tab.value = 'general'
  partial.value = []
  error.value = ''
  busy.value = true
  try {
    const result = await inspectIdentity(props.connection, props.kind, identity.name)
    if (generation === current) display(result)
  } catch (failure) {
    if (generation === current) error.value = errorMessage(failure)
  } finally {
    if (generation === current) busy.value = false
  }
}

function close(): void {
  generation++
  selected.value = undefined
  error.value = ''
  partial.value = []
  editing.value = false
  busy.value = false
}

async function create(draft: PrivateAccessDraft, newPath: string): Promise<void> {
  const current = ++generation
  busy.value = true
  error.value = ''
  partial.value = []
  try {
    const result = await createIdentity(props.connection, props.kind, {
      name: draft.name, path: newPath, members: draft.groups, policies: draft.policies,
    })
    if (current !== generation) return
    creating.value = false
    partial.value = result.failures
    tab.value = draft.groups.length ? 'membership' : draft.policies.length ? 'policies' : 'general'
    display(result.identity)
  } catch (failure) {
    if (current === generation) error.value = errorMessage(failure)
  } finally {
    if (current === generation) {
      busy.value = false
      await reload()
    }
  }
}

async function update(): Promise<void> {
  const identity = selected.value
  if (!identity || readOnly.value || busy.value) return
  const current = ++generation
  const candidate = name.value.trim()
  const newPath = path.value.trim()
  const kind = props.kind
  const values = {
    [`${kind}Name`]: identity.name,
    [`New${kind}Name`]: candidate !== identity.name ? candidate : '',
    NewPath: newPath !== identity.path ? newPath : '',
  }
  busy.value = true
  error.value = ''
  let written = false
  try {
    await writeQuery(props.connection, `Update${kind}`, values)
    written = true
    if (current === generation) {
      display({ ...identity, name: candidate, path: newPath })
      editing.value = false
    }
  } catch (failure) {
    if (current === generation) error.value = errorMessage(failure)
  }
  if (current !== generation) return
  try {
    const result = await inspectIdentity(props.connection, kind, written ? candidate : identity.name)
    if (current === generation) {
      if (written) display(result)
      else selected.value = result
    }
  } catch (failure) {
    if (current === generation) error.value = [error.value, errorMessage(failure)].filter(Boolean).join('\n')
  } finally {
    if (current === generation) {
      busy.value = false
      await reload()
    }
  }
}

async function remove(): Promise<void> {
  if (!selected.value || readOnly.value || busy.value) return
  const current = ++generation
  const target = selected.value.name
  confirming.value = false
  busy.value = true
  error.value = ''
  try {
    await writeQuery(props.connection, `Delete${props.kind}`, { [`${props.kind}Name`]: target })
    if (current === generation) {
      selected.value = undefined
      busy.value = false
      partial.value = []
    }
  } catch (failure) {
    if (current === generation) {
      error.value = errorMessage(failure)
      busy.value = false
    }
  } finally {
    if (current === generation) await reload()
  }
}
</script>

<template>
  <div class="panel-form">
    <template v-if="!selected">
      <div class="stage-heading">
        <h2>IAM {{ kind.toLowerCase() }}s</h2>
        <button class="primary" @click="creating = true; error = ''"><Plus :size="16" /> Create {{ kind.toLowerCase() }}</button>
      </div>
      <form class="inventory-controls" @submit.prevent="reload">
        <label class="toolbar-search">Search {{ kind.toLowerCase() }}s<input v-model="search" type="search" /></label>
        <button :disabled="loading">Search</button>
        <button type="button" :disabled="loading" @click="reload"><RefreshCw :size="15" /> Refresh</button>
      </form>
      <Notice v-if="listError" tone="error">{{ listError }}</Notice>
      <p v-else-if="loading" role="status">Loading {{ kind.toLowerCase() }}s…</p>
      <p v-else-if="!items.length" class="muted">No matching {{ kind.toLowerCase() }}s.</p>
      <div v-for="identity in items" :key="identity.id" class="settings-row">
        <div class="settings-description"><strong>{{ identity.name }}</strong><p>{{ identity.path }} · {{ identity.arn }}</p></div>
        <span v-if="kind === 'User' && managedUser(identity.name)" class="chip">Managed</span>
        <button type="button" :aria-label="`Open ${identity.name}`" @click="open(identity)">Open</button>
      </div>
      <QueryPager :busy="loading" :previous="!!markers.length" :next="!!nextMarker" @previous="previous" @next="next" />
    </template>
    <template v-else>
      <button type="button" :disabled="busy" @click="close"><ArrowLeft :size="15" /> IAM {{ kind.toLowerCase() }}s</button>
      <div class="stage-heading">
        <div><h2>{{ selected.name }}</h2><p>{{ selected.arn }}</p></div>
        <div v-if="!readOnly" class="panel-actions">
          <button type="button" :disabled="busy" @click="display(selected); editing = true; error = ''">Edit {{ kind.toLowerCase() }}</button>
          <button type="button" :disabled="busy" @click="confirming = true">Delete {{ kind.toLowerCase() }}</button>
        </div>
      </div>
      <Notice v-if="partial.length" tone="error">The resource exists. Some assignments need attention. <ul><li v-for="failure in partial" :key="failure">{{ failure }}</li></ul></Notice>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="readOnly">Storeforge manages this identity through <button class="subtle" @click="emit('personal-access')">Personal access</button>.</Notice>
      <WorkspaceTabs v-model="tab" :tabs="tabs" label="IAM resource sections" />
      <FadeRegion>
        <div :key="`${selected.name}:${tab}`">
          <dl v-if="tab === 'general'" class="iam-metadata">
            <dt>IAM path</dt><dd>{{ selected.path || 'Unavailable' }}</dd>
            <dt>Resource ID</dt><dd>{{ selected.id }}</dd><dt>Created</dt><dd>{{ selected.created || 'Unavailable' }}</dd>
          </dl>
          <IamMembership v-else-if="tab === 'membership'" :connection="connection" :kind="kind" :name="selected.name" :read-only="readOnly" />
          <IamKeys v-else-if="tab === 'keys'" :connection="connection" :user="selected" :read-only="readOnly" />
          <IamPolicies v-else :connection="connection" :kind="kind" :name="selected.name" :read-only="readOnly" />
        </div>
      </FadeRegion>
    </template>
    <IamIdentityEditor v-if="creating" :connection="connection" :kind="kind" :busy="busy" :error="error"
      @close="creating = false; error = ''" @save="create" />
    <Modal v-if="editing && selected" :title="`Edit ${kind.toLowerCase()}`" @close="!busy && (editing = false)">
      <form class="modal-form" @submit.prevent="update">
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <label>{{ kind }} name<input v-model="name" required :disabled="busy" /></label>
        <label>IAM path<input v-model="path" required :disabled="busy" /></label>
        <div class="modal-actions">
          <button type="button" :disabled="busy" @click="editing = false">Cancel</button>
          <button class="primary" :disabled="busy || !name.trim() || !path.trim() || (name.trim() === selected.name && path.trim() === selected.path)">Save changes</button>
        </div>
      </form>
    </Modal>
    <ConfirmDialog v-if="confirming && selected" :title="`Delete ${selected.name}?`" description="This deletes the IAM resource from the provider."
      :action="`Delete ${kind.toLowerCase()}`" danger @close="confirming = false" @confirm="remove" />
  </div>
</template>

<style scoped>
.inventory-controls { display: flex; flex-wrap: wrap; gap: 8px; align-items: end; }
.toolbar-search { min-width: 13rem; flex: 1; }
.iam-metadata { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 6px 14px; margin: 0; }
.iam-metadata dd { margin: 0; overflow-wrap: anywhere; }
.settings-description p { overflow-wrap: anywhere; }
</style>
