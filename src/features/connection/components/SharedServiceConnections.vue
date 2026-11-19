<script setup lang="ts">
import { computed, onMounted, reactive, ref, useId, watch } from 'vue'
import { Cable, ExternalLink, Pencil, Plus, RefreshCw, Trash2 } from 'lucide-vue-next'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { sharedServiceApi } from '../api/shared-service'
import {
  sharedServiceNeedsAuthorization, type SharedAction, type SharedContext, type SharedServiceDraft, type SharedServiceProfile,
  type SharedServiceStatus, type SharedWorkspace,
} from '../model/shared-service'

const heading = useId()
const fieldPrefix = useId()
const statuses = ref<SharedServiceStatus[]>([])
const serviceId = ref('')
const selected = computed(() => statuses.value.find(status => status.profile.id === serviceId.value))
const options = computed(() => statuses.value.map(({ profile }) => ({ value: profile.id, label: profile.name })))
const editing = ref<string | null>()
const forget = ref(false)
const draft = reactive<SharedServiceDraft>({ name: '', origin: '' })
const token = ref('')
const busy = ref(false)
const error = ref('')
const contexts = ref<SharedContext[]>()
const workspace = ref<SharedWorkspace>()
let expiryPending = false

onMounted(() => { void run(reload) })

watch(statuses, (value, _, cleanup) => {
  const expiries = value.flatMap(({ session }) => session ? [Date.parse(session.expiresAt)] : [])
  if (!expiries.length) return
  const delay = Math.min(2_147_483_647, Math.max(0, Math.min(...expiries) - Date.now()))
  const timer = window.setTimeout(expire, delay)
  cleanup(() => window.clearTimeout(timer))
})

async function reload(): Promise<void> {
  statuses.value = await sharedServiceApi.list()
  if (!selected.value) select(statuses.value[0]?.profile.id ?? '')
  if (!selected.value?.session) clearContexts()
}

function expire(): void {
  if (busy.value) {
    expiryPending = true
    return
  }
  expireVisibleSessions()
  void run(reload)
}

function expireVisibleSessions(): void {
  if (selected.value?.session && Date.parse(selected.value.session.expiresAt) <= Date.now()) clearContexts()
  statuses.value = statuses.value.map(status => status.session && Date.parse(status.session.expiresAt) <= Date.now()
    ? { ...status, session: null } : status)
}

function clearContexts(): void {
  contexts.value = undefined
  workspace.value = undefined
}

function select(id: string): void {
  serviceId.value = id
  token.value = ''
  error.value = ''
  clearContexts()
}

function edit(profile?: SharedServiceProfile): void {
  editing.value = profile?.id ?? null
  draft.name = profile?.name ?? ''
  draft.origin = profile?.origin ?? ''
  error.value = ''
}

async function run(operation: () => Promise<void>): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    await operation()
  } catch (failure) {
    error.value = errorMessage(failure)
    if (sharedServiceNeedsAuthorization(failure)) {
      clearContexts()
      statuses.value = statuses.value.map(status => status.profile.id === serviceId.value ? { ...status, session: null } : status)
    }
  } finally {
    busy.value = false
    if (expiryPending) {
      expiryPending = false
      expire()
    }
  }
}

function save(): void {
  const id = editing.value
  if (id === undefined) return
  void run(async () => {
    const profile = await sharedServiceApi.save(id, { ...draft })
    editing.value = undefined
    select(profile.id)
    await reload()
  })
}

function authorize(): void {
  if (busy.value) return
  const entered = token.value
  token.value = ''
  const id = serviceId.value
  void run(async () => {
    await sharedServiceApi.connect(id, entered)
    await reload()
  })
}

function readContexts(requested: SharedWorkspace): void {
  const id = serviceId.value
  void run(async () => {
    clearContexts()
    contexts.value = await sharedServiceApi.contexts(id, requested)
    workspace.value = requested
  })
}

function openAction(action: SharedAction): void {
  const id = serviceId.value
  void run(async () => { await sharedServiceApi.open(id, action) })
}

function disconnect(remove = false): void {
  const id = serviceId.value
  forget.value = false
  void run(async () => {
    await sharedServiceApi.disconnect(id, remove)
    clearContexts()
    await reload()
  })
}
</script>

<template>
  <section class="shared-services" :aria-labelledby="heading">
    <header class="settings-toolbar">
      <div>
        <h2 :id="heading">Shared services</h2>
        <p>Connect to a BucketReef service for shared project access.</p>
      </div>
      <div class="workspace-actions">
        <button class="icon-button" type="button" aria-label="Refresh services" :disabled="busy" @click="run(reload)"><RefreshCw :size="16" /></button>
        <button type="button" :disabled="busy" @click="edit()"><Plus :size="16" /> Add shared service</button>
      </div>
    </header>

    <Notice v-if="error && editing === undefined" tone="error">{{ error }}</Notice>
    <FadeRegion>
      <div v-if="selected" :key="selected.profile.id" class="settings-sheet">
        <div class="settings-row shared-service-profile">
          <span class="file-icon"><Cable :size="19" /></span>
          <div class="settings-description">
            <AppSelect :model-value="serviceId" :options="options" aria-label="Shared service profile" :disabled="busy" @update:model-value="select" />
            <p>{{ selected.profile.origin }}</p>
          </div>
          <span class="chip" :class="{ 'chip-active': selected.session }">{{ selected.session ? 'API authorized' : 'API disconnected' }}</span>
          <button class="icon-button subtle" type="button" aria-label="Edit service" :disabled="busy" @click="edit(selected.profile)"><Pencil :size="16" /></button>
          <button class="icon-button subtle" type="button" aria-label="Forget service" :disabled="busy" @click="forget = true"><Trash2 :size="16" /></button>
        </div>
        <div v-if="selected.session" class="settings-row shared-service-profile">
          <div class="settings-description">
            <strong>{{ selected.session.principal.email }}</strong>
            <p>API access expires {{ new Date(selected.session.expiresAt).toLocaleString() }}.</p>
          </div>
          <button type="button" :disabled="busy" @click="disconnect()">Disconnect API</button>
        </div>
        <form v-else class="settings-row shared-service-profile" @submit.prevent="authorize">
          <label class="settings-description" :for="`${fieldPrefix}-token`">
            Scoped API token
            <input :id="`${fieldPrefix}-token`" v-model="token" type="password" autocomplete="off" :disabled="busy" required />
          </label>
          <button class="primary" type="submit" :disabled="busy || !token.trim()">Authorize API</button>
        </form>
        <div class="settings-row shared-service-profile">
          <div class="settings-description"><strong>Service actions</strong><p>Browser actions use the service’s sign-in.</p></div>
          <button v-if="selected.session?.access.manager.available" type="button" :disabled="busy" @click="readContexts('manager')">Manager contexts</button>
          <button v-if="selected.session?.access.browser.available" type="button" :disabled="busy" @click="readContexts('browser')">Browser contexts</button>
          <button type="button" :disabled="busy" @click="openAction('accountSecurity')"><ExternalLink :size="15" /> Account security</button>
          <button v-if="selected.session?.access.admin?.available" type="button" :disabled="busy" @click="openAction('identitySecurity')"><ExternalLink :size="15" /> Identity security</button>
        </div>
        <section v-if="contexts" :aria-label="workspace === 'manager' ? 'Manager contexts' : 'Browser contexts'">
          <div v-for="context in contexts" :key="`${context.kind}:${context.id}`" class="settings-row">
            <div class="settings-description"><strong>{{ context.name }}</strong><p>{{ context.endpointName || context.endpointUrl }}</p></div>
            <span v-if="context.connectionScope" class="chip">{{ context.connectionScope === 'private' ? 'Private connection' : 'Shared connection' }}</span>
          </div>
          <div v-if="!contexts.length" class="settings-row">No contexts available in this workspace.</div>
        </section>
      </div>
    </FadeRegion>

    <Modal v-if="editing !== undefined" :title="editing ? 'Edit shared service' : 'Add shared service'" @close="editing = undefined">
      <form class="modal-form" @submit.prevent="save">
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <label :for="`${fieldPrefix}-name`">Service name<input :id="`${fieldPrefix}-name`" v-model="draft.name" required /></label>
        <label :for="`${fieldPrefix}-origin`">Service origin<input :id="`${fieldPrefix}-origin`" v-model="draft.origin" type="url" placeholder="https://storage.example.org" required /></label>
        <div class="modal-actions">
          <button type="button" :disabled="busy" @click="editing = undefined">Cancel</button>
          <button class="primary" type="submit" :disabled="busy">Save service</button>
        </div>
      </form>
    </Modal>
    <ConfirmDialog v-if="forget && selected" title="Forget shared service?"
      :description="`${selected.profile.name} will be removed from this device.`"
      action="Forget service" danger @close="forget = false" @confirm="disconnect(true)" />
  </section>
</template>

<style scoped>
.shared-services { display: grid; gap: 10px; }
.settings-toolbar { flex-wrap: wrap; }
.settings-toolbar h2 { margin: 0; font-size: 17px; }
.settings-toolbar p, .settings-description p { overflow-wrap: anywhere; }
.shared-service-profile { flex-wrap: wrap; }
.shared-service-profile .settings-description { min-width: min(220px, 100%); }
.shared-service-profile input { display: block; max-width: 350px; }
</style>
