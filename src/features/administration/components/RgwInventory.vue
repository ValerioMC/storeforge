<script setup lang="ts">
import { computed, onScopeDispose, ref } from 'vue'
import { ArrowLeft, Download, Plus, RefreshCw } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import type { Connection } from '../../connection/model/connection'
import { changeRgwIdentity, listRgwIdentities, readRgwAccount, readRgwAccountUsage, readRgwUser } from '../api/rgw'
import { useQueryPage } from '../composables/useQueryPage'
import { rgwTargetKey, rgwTargetLabel, rgwUserLabel, type RgwAccount, type RgwChange, type RgwKind, type RgwQuota, type RgwTarget, type RgwUser } from '../model/rgw'
import QueryPager from './QueryPager.vue'
import CephActionDialog from './CephActionDialog.vue'
import RgwAccessKeys from './RgwAccessKeys.vue'
import RgwIdentityEditor from './RgwIdentityEditor.vue'
import RgwImport from './RgwImport.vue'
import RgwQuotaEditor from './RgwQuotaEditor.vue'

const props = defineProps<{ connection: string; kind: RgwKind; managed: boolean; cephProfile?: Connection }>()
const emit = defineEmits<{ open: [profile: Connection] }>()
const { items, markers, nextMarker, loading, error: listError, search, next, previous, reload } = useQueryPage(
  () => [props.connection, props.kind, props.managed],
  marker => listRgwIdentities(props.connection, props.kind, props.managed, marker),
  row => [rgwTargetLabel(row.target), row.root ? rgwUserLabel(row.root) : ''].join(' '),
  10,
)
const selected = ref<RgwTarget>()
const related = ref<RgwTarget>()
const account = ref<RgwAccount>()
const user = ref<RgwUser>()
const tab = ref('overview')
const removing = ref(false)
const detail = computed(() => account.value ?? user.value)
const deleteSelection = computed(() => user.value
  ? { kind: 'deleteUser' as const, identity: user.value.identity, account: user.value.account }
  : account.value ? { kind: 'deleteAccount' as const, id: account.value.id, tenant: account.value.tenant } : undefined)
const editor = ref<'create' | 'edit' | 'import' | 'quota' | 'bucketQuota'>()
const busy = ref(false)
const reading = ref(false)
const error = ref('')
const detailError = ref('')
const usageError = ref('')
const readingUsage = ref(false)
const issue = ref('')
let generation = 0
let disposed = false
onScopeDispose(() => { generation++; disposed = true })
const detailTabs = computed(() => user.value
  ? [{ value: 'overview', label: 'Overview' }, { value: 'keys', label: 'RGW keys' }]
  : [{ value: 'overview', label: 'Overview' }])
const title = computed(() => `${props.managed ? 'Managed RGW' : 'Cluster RGW'} ${props.kind}s`)
const quotas = computed(() => [
  { kind: 'quota', label: user.value ? 'User quota' : 'Account quota', quota: detail.value?.quota ?? null },
  { kind: 'bucketQuota', label: 'Bucket quota', quota: detail.value?.bucketQuota ?? null },
] as const)

async function read(): Promise<void> {
  const target = selected.value
  if (!target) return
  const request = ++generation
  reading.value = true
  detailError.value = ''
  try {
    if (target.kind === 'account') {
      const result = await readRgwAccount(props.connection, target.id)
      if (request === generation) account.value = result
    } else {
      const result = await readRgwUser(props.connection, target.identity)
      if (request === generation) user.value = result
    }
  } catch (failure) {
    if (request === generation) detailError.value = errorMessage(failure)
  } finally {
    if (request === generation) reading.value = false
  }
}

async function loadUsage(): Promise<void> {
  const current = account.value
  if (!current || readingUsage.value) return
  const request = generation
  readingUsage.value = true
  usageError.value = ''
  try {
    const usage = await readRgwAccountUsage(props.connection, current.id)
    if (request === generation) account.value = { ...current, usage }
  } catch (failure) {
    if (request === generation) usageError.value = errorMessage(failure)
  } finally {
    if (request === generation) readingUsage.value = false
  }
}

function open(target: RgwTarget): void {
  account.value = undefined
  user.value = undefined
  selected.value = target
  usageError.value = ''
  readingUsage.value = false
  tab.value = 'overview'
  void read()
}

function openOwner(target: RgwTarget): void {
  related.value = selected.value
  open(target)
}

function back(): void {
  if (related.value) {
    const target = related.value
    related.value = undefined
    open(target)
    return
  }
  generation++
  selected.value = undefined
  detailError.value = ''
  readingUsage.value = false
  reading.value = false
}

function startEditor(value: NonNullable<typeof editor.value>): void {
  editor.value = value
  error.value = ''
  issue.value = ''
}

function saveQuota(quota: RgwQuota): void {
  if (selected.value) void save({ action: 'setQuota', target: selected.value, bucket: editor.value === 'bucketQuota', quota })
}

async function save(change: RgwChange): Promise<void> {
  if (busy.value) return
  const currentEditor = editor.value
  busy.value = true
  error.value = ''
  issue.value = ''
  try {
    const result = await changeRgwIdentity(props.connection, change)
    if (disposed) return
    editor.value = undefined
    issue.value = result.issue ?? ''
    if (result.target) open(result.target)
    if (currentEditor === 'create' || currentEditor === 'import') await reload()
  } catch (failure) {
    if (!disposed) error.value = errorMessage(failure)
  } finally {
    if (!disposed && selected.value && currentEditor !== 'create') await read()
    busy.value = false
  }
}

function quotaLimit(value: number | null | undefined): string {
  if (value == null) return 'Unavailable'
  return value < 0 ? 'Unlimited' : value.toLocaleString()
}

function finishRemoval(changed: boolean): void {
  removing.value = false
  if (!changed) return
  back()
  void reload()
}
</script>

<template>
  <div class="panel-form rgw-inventory">
    <Notice v-if="issue" tone="error">{{ issue }}</Notice>
    <template v-if="!selected">
      <div class="stage-heading">
        <div><h2>{{ title }}</h2><p>{{ managed ? 'Identities registered in this Storeforge workspace.' : 'Identities reported by the selected Ceph cluster.' }}</p></div>
        <div class="panel-actions">
          <button v-if="managed" @click="startEditor('import')"><Download :size="15" /> Import {{ kind }}s</button>
          <button class="primary" @click="startEditor('create')"><Plus :size="15" /> Create {{ kind }}</button>
        </div>
      </div>
      <form class="rgw-toolbar" @submit.prevent="reload">
        <label>Search {{ kind }}s<input v-model="search" type="search" /></label>
        <button :disabled="loading">Search</button>
        <button type="button" :disabled="loading" @click="reload"><RefreshCw :size="15" /> Refresh</button>
      </form>
      <Notice v-if="listError" tone="error">{{ listError }}</Notice>
      <p v-if="loading" role="status">Loading RGW {{ kind }}s…</p>
      <p v-else-if="!items.length" class="muted">No {{ managed ? 'managed' : 'matching' }} {{ kind }}s.</p>
      <div v-else class="object-table-frame">
        <table class="object-table rgw-table">
          <thead><tr><th scope="col">{{ kind === 'account' ? 'Account ID' : 'UID' }}</th><th scope="col">{{ kind === 'account' ? 'Registered root' : 'Tenant' }}</th><th scope="col">Registration</th></tr></thead>
          <tbody>
            <tr v-for="row in items" :key="rgwTargetKey(row.target)">
              <td><button type="button" class="subtle" :aria-label="`Open ${rgwTargetLabel(row.target)}`" @click="open(row.target)">{{ row.target.kind === 'user' ? row.target.identity.uid : row.target.id }}</button></td>
              <td>{{ row.target.kind === 'user' ? row.target.identity.tenant || 'Default' : row.root ? rgwUserLabel(row.root) : '—' }}</td>
              <td><span class="chip">{{ row.managed ? 'Managed' : 'Cluster only' }}</span></td>
            </tr>
          </tbody>
        </table>
      </div>
      <QueryPager :busy="loading" :previous="!!markers.length" :next="!!nextMarker" @previous="previous" @next="next" />
    </template>
    <template v-else>
      <div class="stage-heading">
        <button type="button" :disabled="busy" @click="back"><ArrowLeft :size="15" /> {{ related ? rgwTargetLabel(related) : title }}</button>
        <div class="panel-actions">
          <button :disabled="reading || readingUsage || busy" @click="read"><RefreshCw :size="15" /> Refresh details</button>
          <button v-if="detail" :disabled="reading || readingUsage || busy" @click="startEditor('edit')">Edit {{ selected.kind }}</button>
          <button v-if="cephProfile && deleteSelection" type="button" class="danger-outline" :disabled="reading || readingUsage || busy" @click="removing = true">Delete {{ selected.kind }}</button>
        </div>
      </div>
      <Notice v-if="detailError" tone="error">{{ detailError }}</Notice>
      <p v-if="reading && !detail" role="status">Reading RGW {{ selected.kind }}…</p>
      <template v-if="detail">
        <div class="stage-heading"><div><h2>{{ account?.name || user?.displayName || rgwTargetLabel(selected) }}</h2><p>{{ rgwTargetLabel(selected) }}</p></div></div>
        <WorkspaceTabs v-if="user" v-model="tab" :tabs="detailTabs" label="RGW resource sections" />
        <FadeRegion>
          <div v-if="tab === 'overview'" key="overview" class="panel-form">
            <dl class="rgw-metadata">
              <dt>Tenant</dt><dd>{{ (account?.tenant ?? user?.identity.tenant) || 'Default' }}</dd>
              <dt>Email</dt><dd>{{ detail.email || '—' }}</dd>
              <template v-if="account">
                <dt>Registered root</dt><dd><button v-if="account.root" class="subtle" @click="openOwner({ kind: 'user', identity: account.root })">{{ rgwUserLabel(account.root) }}</button><span v-else>—</span></dd>
                <dt>Users</dt><dd>{{ account.userCount ?? 'Unavailable' }}</dd>
                <dt>User limit</dt><dd>{{ quotaLimit(account.limits.maxUsers) }}</dd>
                <dt>Bucket limit</dt><dd>{{ quotaLimit(account.limits.maxBuckets) }}</dd>
                <dt>Role limit</dt><dd>{{ quotaLimit(account.limits.maxRoles) }}</dd>
                <dt>Group limit</dt><dd>{{ quotaLimit(account.limits.maxGroups) }}</dd>
                <dt>Keys per user</dt><dd>{{ quotaLimit(account.limits.maxAccessKeys) }}</dd>
              </template>
              <template v-if="user">
                <dt>Owning account</dt><dd><button v-if="user.account" class="subtle" @click="openOwner({ kind: 'account', id: user.account })">{{ user.account }}</button><span v-else>Standalone</span></dd>
                <dt>Account root</dt><dd>{{ user.accountRoot == null ? 'Unavailable' : user.accountRoot ? 'Yes' : 'No' }}</dd>
                <dt>User state</dt><dd>{{ user.suspended == null ? 'Unavailable' : user.suspended ? 'Suspended' : 'Active' }}</dd>
                <dt>Bucket limit</dt><dd>{{ quotaLimit(user.maxBuckets) }}</dd>
                <dt>Access keys</dt><dd>{{ user.keys?.length ?? 'Unavailable' }}</dd>
              </template>
              <dt>Storage bytes</dt><dd>{{ detail.usage?.bytes?.toLocaleString() ?? 'Unavailable' }}</dd>
              <dt>Objects</dt><dd>{{ detail.usage?.objects?.toLocaleString() ?? 'Unavailable' }}</dd>
              <dt>Buckets</dt><dd>{{ detail.usage?.buckets?.toLocaleString() ?? 'Unavailable' }}</dd>
            </dl>
            <div v-if="account" class="panel-actions">
              <button :disabled="reading || readingUsage || busy" @click="loadUsage">{{ readingUsage ? 'Reading account usage…' : 'Load account usage' }}</button>
            </div>
            <Notice v-if="usageError" tone="error">{{ usageError }}</Notice>
            <div v-for="entry in quotas" :key="entry.kind" class="settings-row">
              <div class="settings-description">
                <strong>{{ entry.label }}</strong>
                <p v-if="entry.quota">{{ entry.quota.enabled ? 'Enabled' : 'Disabled' }} · {{ quotaLimit(entry.quota.maxBytes) }} bytes · {{ quotaLimit(entry.quota.maxObjects) }} objects</p>
                <p v-else>Quota details unavailable</p>
              </div>
              <button :disabled="reading || readingUsage || busy" @click="startEditor(entry.kind)">Edit {{ entry.label.toLowerCase() }}</button>
            </div>
          </div>
          <RgwAccessKeys v-else-if="user" key="keys" :connection="connection" :owner="user" scope="inventory" :loading="reading" :reload="read" @open="emit('open', $event)" />
        </FadeRegion>
      </template>
    </template>
    <RgwIdentityEditor v-if="editor === 'create' || editor === 'edit'" :connection="connection" :kind="editor === 'edit' ? selected?.kind ?? kind : kind" :managed="managed"
      :account="editor === 'edit' ? account : undefined" :user="editor === 'edit' ? user : undefined" :busy="busy" :error="error" @close="editor = undefined" @save="save" />
    <RgwImport v-if="editor === 'import'" :connection="connection" :kind="kind" :busy="busy" :error="error" @close="editor = undefined" @save="save" />
    <CephActionDialog v-if="removing && cephProfile && deleteSelection" :profile="cephProfile" :selection="deleteSelection" @close="finishRemoval" />
    <RgwQuotaEditor v-if="editor === 'quota' || editor === 'bucketQuota'"
      :label="editor === 'quota' ? quotas[0].label : quotas[1].label" :quota="editor === 'quota' ? quotas[0].quota : quotas[1].quota"
      :busy="busy" :error="error" @close="editor = undefined" @save="saveQuota" />
  </div>
</template>

<style scoped>
.rgw-toolbar { display: flex; align-items: end; gap: 8px; flex-wrap: wrap; }
.rgw-toolbar label { flex: 1; min-width: 12rem; }
.rgw-table td, .rgw-table th { padding-inline: 12px; overflow-wrap: anywhere; }
.rgw-metadata { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 8px 16px; margin: 0; }
.rgw-metadata dd { margin: 0; overflow-wrap: anywhere; }
</style>
