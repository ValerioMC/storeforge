<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { Download, ExternalLink, RefreshCw, ShieldCheck } from 'lucide-vue-next'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { sharedServiceApi } from '../../connection/api/shared-service'
import { sharedServiceNeedsAuthorization, type SharedAction, type SharedServiceStatus } from '../../connection/model/shared-service'
import { governanceApi } from '../api/platform-governance'
import {
  accessRights, accessScopes, grantKinds, type AccessFilters, type AccessReviewRow, type GovernancePage,
  type GroupDraft, type PlatformConnection, type PlatformGroup, type PrivateConnection,
} from '../model/platform-governance'
import AccessReviewTable from './AccessReviewTable.vue'
import PlatformGroups from './PlatformGroups.vue'
import QueryPager from './QueryPager.vue'

type PlatformTab = 'access' | 'users' | 'groups' | 'shared' | 'private' | 'security'
type Editor = { kind: 'group'; id: number | null } | { kind: 'connection'; id: number }
type Inventory =
  | { tab: 'access'; page: GovernancePage<AccessReviewRow> }
  | { tab: 'groups'; page: GovernancePage<PlatformGroup> }
  | { tab: 'shared'; page: GovernancePage<PlatformConnection> }
  | { tab: 'private'; items: PrivateConnection[] }

const emit = defineEmits<{ connections: [] }>()
const tabs: { value: PlatformTab; label: string }[] = [
  { value: 'access', label: 'Access review' }, { value: 'users', label: 'Users' }, { value: 'groups', label: 'Groups' },
  { value: 'shared', label: 'Shared connections' }, { value: 'private', label: 'Private connections' }, { value: 'security', label: 'Security' },
]
const securityActions: { title: string; detail: string; action: SharedAction }[] = [
  { title: 'Account preferences', detail: 'Name, preferences and avatar', action: 'accountProfile' },
  { title: 'Account security', detail: 'Active sessions, passkeys, recovery codes and sign-in methods', action: 'accountSecurity' },
  { title: 'Identity security', detail: 'Manage user sessions and decide identity-link requests', action: 'identitySecurity' },
  { title: 'Scoped API tokens', detail: 'Create, list and revoke tokens for service access', action: 'apiTokens' },
  { title: 'Authentication providers', detail: 'Configure OpenID Connect and LDAP', action: 'authenticationSettings' },
]
const inventoryActions: Partial<Record<PlatformTab, { action: SharedAction; label: string }>> = {
  access: { action: 'accessReview', label: 'Open access review in browser' },
  users: { action: 'adminUsers', label: 'Open users and directory import' },
  groups: { action: 'adminGroups', label: 'Manage groups in browser' },
  shared: { action: 'sharedConnections', label: 'Manage shared connections in browser' },
  private: { action: 'privateConnections', label: 'Manage private connections in browser' },
}
const scopeOptions = filterOptions(accessScopes, 'All resources')
const sourceOptions = filterOptions(grantKinds, 'All sources')
const rightOptions = filterOptions(accessRights, 'All rights')
const profiles = ref<SharedServiceStatus[]>([])
const serviceId = ref('')
const serviceOptions = computed(() => profiles.value.map(({ profile }) => ({ value: profile.id, label: profile.name })))
const selected = computed(() => profiles.value.find(({ profile }) => profile.id === serviceId.value))
const tab = ref<PlatformTab>('access')
const inventory = ref<Inventory>()
const page = ref(1)
const pageResult = computed(() => inventory.value && 'page' in inventory.value ? inventory.value.page : undefined)
const search = ref('')
const scope = ref<keyof typeof accessScopes | ''>('')
const source = ref<keyof typeof grantKinds | ''>('')
const right = ref<keyof typeof accessRights | ''>('')
const groupFilter = ref<PlatformGroup>()
const appliedFilters = ref<AccessFilters>({})
const appliedSearch = ref<string>()
const loading = ref(false)
const actionPending = ref(false)
const error = ref('')
const editor = ref<Editor>()
const draft = reactive<GroupDraft>({ name: '', description: null })
const apiAccess = computed(() => {
  const session = selected.value?.session
  if (!session || Date.parse(session.expiresAt) <= Date.now()) return false
  if (tab.value === 'private') return true
  return !!session.access.admin?.available && session.principal.role !== 'ui_user'
})
const readsInventory = computed(() => tab.value !== 'users' && tab.value !== 'security')
const browserAction = computed(() => inventoryActions[tab.value])
const draftTitle = computed(() => editor.value?.kind === 'connection' ? 'Rename shared connection' : editor.value?.id ? 'Edit group details' : 'Create group')
let readSequence = 0

onMounted(() => { void refresh() })

watch(selected, (status, _, cleanup) => {
  if (!status?.session) return
  const expiry = Date.parse(status.session.expiresAt)
  const delay = Math.min(2_147_483_647, Math.max(0, expiry - Date.now()))
  const timer = window.setTimeout(() => {
    if (expiry <= Date.now()) clearAuthorization()
    void refresh()
  }, delay)
  cleanup(() => window.clearTimeout(timer))
})

function filterOptions<Key extends string>(labels: Record<Key, string>, label: string): { value: Key | ''; label: string }[] {
  return [{ value: '', label }, ...(Object.keys(labels) as Key[]).map(value => ({ value, label: labels[value] }))]
}

function clearAuthorization(): void {
  readSequence++
  loading.value = false
  inventory.value = undefined
  profiles.value = profiles.value.map(status => status.profile.id === serviceId.value ? { ...status, session: null } : status)
}

async function reloadProfiles(): Promise<void> {
  profiles.value = await sharedServiceApi.list()
  if (!selected.value) {
    serviceId.value = (profiles.value.find(status => status.session?.access.admin?.available) ?? profiles.value[0])?.profile.id ?? ''
    resetFilters()
  }
}

async function refresh(): Promise<void> {
  error.value = ''
  try {
    await reloadProfiles()
    await loadInventory()
  } catch (failure) {
    showError(failure)
  }
}

function selectService(id: string): void {
  serviceId.value = id
  resetFilters()
  void loadInventory()
}

function switchTab(value: PlatformTab): void {
  if (tab.value === value) return
  tab.value = value
  resetFilters()
  void loadInventory()
}

function resetFilters(): void {
  page.value = 1
  search.value = ''
  scope.value = ''
  source.value = ''
  right.value = ''
  groupFilter.value = undefined
  appliedFilters.value = {}
  appliedSearch.value = undefined
}

function applyFilters(): void {
  appliedSearch.value = search.value.trim() || undefined
  appliedFilters.value = {
    search: appliedSearch.value, scope: scope.value || undefined, source: source.value || undefined,
    right: right.value || undefined, groupId: groupFilter.value?.id,
  }
  changePage(1)
}

function reviewGroup(group?: PlatformGroup): void {
  if (group) {
    tab.value = 'access'
    resetFilters()
  }
  groupFilter.value = group
  applyFilters()
}

function changePage(value: number): void {
  page.value = value
  void loadInventory()
}

async function loadInventory(): Promise<void> {
  const read = ++readSequence
  inventory.value = undefined
  error.value = ''
  loading.value = false
  if (!apiAccess.value || !readsInventory.value) return
  loading.value = true
  try {
    const result = await fetchInventory(serviceId.value, tab.value)
    if (read === readSequence) inventory.value = result
  } catch (failure) {
    if (read === readSequence) showError(failure)
  } finally {
    if (read === readSequence) loading.value = false
  }
}

async function fetchInventory(id: string, currentTab: PlatformTab): Promise<Inventory | undefined> {
  const query = { page: page.value, search: appliedSearch.value }
  if (currentTab === 'access') return { tab: currentTab, page: await governanceApi.access(id, query.page, appliedFilters.value) }
  if (currentTab === 'groups') return { tab: currentTab, page: await governanceApi.groups(id, query) }
  if (currentTab === 'shared') return { tab: currentTab, page: await governanceApi.connections(id, query) }
  if (currentTab === 'private') return { tab: currentTab, items: await governanceApi.privateConnections(id) }
  return undefined
}

function showError(failure: unknown): void {
  error.value = errorMessage(failure)
  if (sharedServiceNeedsAuthorization(failure)) clearAuthorization()
}

async function runAction(operation: (id: string) => Promise<unknown>): Promise<void> {
  if (actionPending.value || !serviceId.value) return
  actionPending.value = true
  error.value = ''
  try {
    await operation(serviceId.value)
  } catch (failure) {
    showError(failure)
  } finally {
    actionPending.value = false
  }
}

function openAction(action: SharedAction): void {
  void runAction(id => sharedServiceApi.open(id, action))
}

function exportAccess(): void {
  void runAction(id => governanceApi.exportAccess(id, appliedFilters.value))
}

function editGroup(group?: PlatformGroup): void {
  editor.value = { kind: 'group', id: group?.id ?? null }
  draft.name = group?.name ?? ''
  draft.description = group?.description ?? null
  error.value = ''
}

function renameConnection(connection: PlatformConnection): void {
  editor.value = { kind: 'connection', id: connection.id }
  draft.name = connection.name
  draft.description = null
  error.value = ''
}

function saveEditor(): void {
  const editing = editor.value
  if (!editing) return
  void runAction(async id => {
    if (editing.kind === 'group') await governanceApi.saveGroup(id, editing.id, { ...draft })
    else await governanceApi.renameConnection(id, editing.id, draft.name)
    editor.value = undefined
    await loadInventory()
  })
}
</script>

<template>
  <section class="workspace-page governance">
    <WorkspaceHeader title="Platform governance" description="Review access and manage identities on a shared service.">
      <template #actions>
        <button v-if="selected" type="button" :disabled="loading || actionPending" @click="refresh"><RefreshCw :size="15" /> Refresh</button>
        <button type="button" @click="emit('connections')">Service connections</button>
      </template>
    </WorkspaceHeader>
    <Notice v-if="error && !editor" tone="error">{{ error }}</Notice>
    <EmptyState v-if="!selected" :icon="ShieldCheck" title="Choose a shared service" description="Save a service origin in Connections to access platform governance.">
      <button type="button" @click="emit('connections')">Open Connections</button>
    </EmptyState>
    <template v-else>
      <div class="settings-sheet service-context">
        <div class="settings-row">
          <div class="settings-description">
            <label for="governance-service">Shared service</label>
            <AppSelect id="governance-service" :model-value="serviceId" :options="serviceOptions" :disabled="actionPending" @update:model-value="selectService" />
          </div>
          <div class="settings-description">
            <strong>{{ selected.profile.origin }}</strong>
            <p>{{ selected.session?.principal.email || 'API disconnected' }}</p>
          </div>
        </div>
      </div>
      <WorkspaceTabs label="Governance views" :model-value="tab" :tabs="tabs" @update:model-value="switchTab" />
      <FadeRegion>
        <div :key="tab" class="governance-view">
          <div v-if="tab === 'security'" class="settings-sheet">
            <div v-for="action in securityActions" :key="action.title" class="settings-row">
              <div class="settings-description"><strong>{{ action.title }}</strong><p>{{ action.detail }}</p></div>
              <button type="button" :disabled="actionPending" :aria-label="`Open ${action.title}`" @click="openAction(action.action)">
                <ExternalLink :size="15" /> Open in browser
              </button>
            </div>
          </div>
          <template v-else>
            <div class="settings-toolbar governance-toolbar">
              <form v-if="readsInventory && apiAccess && tab !== 'private'" class="governance-filters" @submit.prevent="applyFilters">
                <input v-model="search" type="search" aria-label="Search governance records" placeholder="Search" />
                <template v-if="tab === 'access'">
                  <AppSelect v-model="scope" :options="scopeOptions" aria-label="Resource scope" />
                  <AppSelect v-model="source" :options="sourceOptions" aria-label="Grant source" />
                  <AppSelect v-model="right" :options="rightOptions" aria-label="Effective right" />
                </template>
                <button type="submit" :disabled="actionPending">Apply filters</button>
              </form>
              <div class="workspace-actions">
                <button v-if="tab === 'access' && apiAccess" type="button" :disabled="loading || actionPending" @click="exportAccess"><Download :size="15" /> Export CSV</button>
                <button v-if="tab === 'groups' && apiAccess" type="button" :disabled="actionPending" @click="editGroup()">Create group</button>
                <button v-if="browserAction" type="button" :disabled="actionPending" @click="openAction(browserAction.action)"><ExternalLink :size="15" /> {{ browserAction.label }}</button>
              </div>
            </div>
            <p v-if="tab === 'users'" class="governance-caption">Use the service’s browser session to list, create, edit, activate or import users and manage their grants.</p>
            <p v-else-if="!apiAccess" class="governance-caption">Authorize a token with {{ tab === 'private' ? 'browser:read' : 'admin:read' }} and workspace access to show these records here.</p>
            <div v-if="tab === 'access' && groupFilter" class="workspace-actions">
              <span class="chip">{{ groupFilter.name }}</span><button class="subtle" type="button" @click="reviewGroup()">Clear group filter</button>
            </div>
            <p v-if="loading" class="governance-caption" role="status">Reading service records…</p>
            <template v-else-if="inventory">
              <div v-if="inventory.tab === 'access' && inventory.page.items.length" class="settings-sheet">
                <AccessReviewTable :rows="inventory.page.items" :busy="actionPending" @review="openAction" />
              </div>
              <PlatformGroups v-else-if="inventory.tab === 'groups' && inventory.page.items.length" :groups="inventory.page.items" :busy="actionPending"
                @edit="editGroup" @grants="group => openAction({ reviewGroup: { id: group.id, search: group.name } })" @review="reviewGroup" />
              <div v-else-if="inventory.tab === 'shared' && inventory.page.items.length" class="settings-sheet">
                <div v-for="connection in inventory.page.items" :key="connection.id" class="settings-row">
                  <div class="settings-description">
                    <strong>{{ connection.name }}</strong>
                    <p>{{ connection.endpointUrl }}</p>
                    <p v-if="connection.createdByFullName || connection.createdByEmail">Created by {{ connection.createdByFullName || connection.createdByEmail }}</p>
                  </div>
                  <span class="chip">Service managed</span>
                  <span class="chip">{{ !connection.isActive ? 'Inactive' : connection.executionStatus === 'ready' ? 'Active' : 'Needs remediation' }}</span>
                  <button type="button" :disabled="actionPending" @click="renameConnection(connection)">Rename</button>
                </div>
              </div>
              <div v-else-if="inventory.tab === 'private' && inventory.items.length" class="settings-sheet">
                <div v-for="connection in inventory.items" :key="connection.id" class="settings-row">
                  <div class="settings-description"><strong>{{ connection.name }}</strong><p>{{ connection.endpointUrl }}</p></div>
                  <span v-if="connection.serverManaged" class="chip">Service managed</span>
                  <span class="chip">Private to {{ selected.session?.principal.email }}</span>
                  <span class="chip">{{ connection.isActive ? 'Active' : 'Inactive' }}</span>
                </div>
              </div>
              <p v-else class="governance-caption">No records match this view.</p>
              <QueryPager v-if="pageResult" :busy="actionPending" :previous="pageResult.page > 1" :next="pageResult.hasNext"
                @previous="changePage(page - 1)" @next="changePage(page + 1)" />
            </template>
          </template>
        </div>
      </FadeRegion>
    </template>
    <Modal v-if="editor" :title="draftTitle" @close="editor = undefined">
      <form class="modal-form" @submit.prevent="saveEditor">
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <label>Name<input v-model="draft.name" required /></label>
        <label v-if="editor.kind === 'group'">Description<textarea v-model="draft.description" /></label>
        <div class="modal-actions">
          <button type="button" :disabled="actionPending" @click="editor = undefined">Cancel</button>
          <button class="primary" type="submit" :disabled="actionPending || !draft.name.trim()">Save</button>
        </div>
      </form>
    </Modal>
  </section>
</template>

<style scoped>
.governance-view { display: grid; gap: 10px; min-width: 0; }
.governance-caption { margin: 0; color: var(--muted); }
.governance-filters, .workspace-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
.governance-filters { flex: 1 1 635px; }
.governance-toolbar > .workspace-actions { margin-inline-start: auto; }
.governance-filters input { width: min(175px, 100%); }
.governance .settings-row { flex-wrap: wrap; }
.governance .settings-description { min-width: min(180px, 100%); }
.governance .settings-description p, .governance .settings-description strong { overflow-wrap: anywhere; }
.service-context .settings-row { gap: 10px 20px; }
.service-context label { display: block; margin-bottom: 3px; color: var(--muted); }
.service-context .settings-description:first-child { max-width: 275px; }
</style>
