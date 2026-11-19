<script setup lang="ts">
import { computed, ref, shallowRef, watch, watchEffect } from 'vue'
import { ChevronLeft, ChevronRight, ClipboardList, Columns3, Database, Download, Filter, RefreshCw, Server } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import WorkspaceMetric from '../../../components/shell/WorkspaceMetric.vue'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import CephIndexBatch from '../../administration/components/CephIndexBatch.vue'
import { batchIndexTargets } from '../../administration/model/ceph-operations'
import { providerNames, type Connection } from '../../connection/model/connection'
import { saveBucketSelection } from '../api/workbench'
import { useBucketClassifications } from '../composables/useBucketClassifications'
import { useBucketWorkbench } from '../composables/useBucketWorkbench'
import type { Configuration } from '../model/bucket'
import type { CampaignSnapshot } from '../model/campaign'
import { adjacentRow, defaultColumns, exportWorkbenchRows, filterWorkbenchRows, focusedRow, rowCell, workbenchColumns,
  type ExportFormat, type InventorySource, type WorkbenchFilter, type WorkbenchRow } from '../model/workbench'
import BucketCampaign from './BucketCampaign.vue'
import BucketWorkbenchDetail from './BucketWorkbenchDetail.vue'
import BucketWorkbenchFilters from './BucketWorkbenchFilters.vue'

const props = defineProps<{ profiles: Connection[]; source: InventorySource; pane: 'overview' | 'buckets'; target?: string; initialBucket?: string; compact?: boolean; link: string }>()
const emit = defineEmits<{ overview: []; inventory: []; inspect: [key?: string]; connections: [] }>()
const profiles = computed(() => props.profiles)
const { snapshots, rows, busy, refresh, enrich, reading, invalidate, applyResult, quotaPending, quotaError } = useBucketWorkbench(profiles, props.source)
const { tags, saveTags } = useBucketClassifications(props.source)
const details = ref<InstanceType<typeof BucketWorkbenchDetail>>()
const campaign = ref<InstanceType<typeof BucketCampaign>>()
const showCampaign = ref(false)
const showIndexBatch = ref(false)
const clipboard = shallowRef<CampaignSnapshot>()
const query = ref(props.initialBucket ?? '')
const match = ref<'contains' | 'exact'>(props.initialBucket ? 'exact' : 'contains')
const context = ref('')
const filters = ref<WorkbenchFilter[]>([])
const selected = ref(new Set<string>())
const columns = ref<string[]>(defaultColumns)
const page = ref(1)
const showFilters = ref(false)
const exportFormat = ref<ExportFormat>('csv')
const exporting = ref(false)
const actionError = ref('')
const pageSize = 30
const tabs = [{ value: 'overview', label: 'Overview' }, { value: 'buckets', label: 'Bucket inventory' }] as const
const searchModes = [{ value: 'contains', label: 'Contains' }, { value: 'exact', label: 'Exact match' }] as const
const exportFormats = [{ value: 'csv', label: 'CSV' }, { value: 'json', label: 'JSON' }, { value: 'text', label: 'Plain text' }] as const
const visibleColumns = computed(() => workbenchColumns.filter(column => columns.value.includes(column.id)))
const contextOptions = computed(() => [{ value: '', label: 'All contexts' }, ...profiles.value.map(profile => ({ value: profile.id, label: profile.name }))])
const featureFilters = computed(() => filters.value.filter(filter => workbenchColumns.some(column => column.id === filter.field && (column.section || column.quota))))
const requiredQuota = computed(() => featureFilters.value.some(filter => workbenchColumns.some(column => column.id === filter.field && column.quota)))
const optionalQuota = computed(() => visibleColumns.value.some(column => column.quota))
const baseRows = computed(() => filterWorkbenchRows(rows.value, query.value, match.value, context.value,
  filters.value.filter(filter => !featureFilters.value.includes(filter)), reading, tags))
const requiredFeatures = computed(() => new Set(filters.value.flatMap(filter => {
  const section = workbenchColumns.find(column => column.id === filter.field)?.section
  return section ? [section] : []
})))
const optionalFeatures = computed(() => new Set(visibleColumns.value.flatMap(column => column.section ? [column.section] : [])))
const filteredRows = computed(() => filterWorkbenchRows(baseRows.value, '', 'contains', '', featureFilters.value, reading, tags))
const pageCount = computed(() => Math.max(1, Math.ceil(filteredRows.value.length / pageSize)))
const pageRows = computed(() => filteredRows.value.slice((page.value - 1) * pageSize, page.value * pageSize))
const chosen = computed(() => rows.value.filter(row => selected.value.has(row.key)))
const cephProfile = computed(() => props.source === 'cephAdmin' && profiles.value[0]?.id !== 'demo' ? profiles.value[0] : undefined)
const indexTargets = computed(() => cephProfile.value ? batchIndexTargets(chosen.value, cephProfile.value) : [])
const everyPageRow = computed(() => !!pageRows.value.length && pageRows.value.every(row => selected.value.has(row.key)))
const selectedOnPage = computed(() => pageRows.value.some(row => selected.value.has(row.key)))
const focused = computed(() => focusedRow(rows.value, props.target))
const previous = computed(() => props.target ? adjacentRow(filteredRows.value, props.target, -1) : undefined)
const next = computed(() => props.target ? adjacentRow(filteredRows.value, props.target, 1) : undefined)
const coveragePending = computed(() => baseRows.value.some(row => incomplete(row, requiredFeatures.value) || requiredQuota.value && quotaPending(row)))
const exportPending = computed(() => chosen.value.some(row => incomplete(row, optionalFeatures.value) || optionalQuota.value && quotaPending(row)))
const dirty = computed(() => !!details.value?.dirty)
const editing = computed(() => !!details.value?.busy || !!campaign.value?.busy)
const contexts = computed(() => profiles.value.map(profile => ({ profile, snapshot: snapshots.get(profile.id) })))

watch(() => props.initialBucket, bucket => {
  if (bucket) {
    query.value = bucket
    match.value = 'exact'
  }
})
watch([query, match, context, filters], () => { page.value = 1 })
watch(pageCount, count => { page.value = Math.min(page.value, count) })
watch(() => profiles.value.map(profile => [profile.id, profile.revision] as const), (current, previousProfiles) => {
  if (context.value && !current.some(([id]) => id === context.value)) context.value = ''
  const revisions = new Map(previousProfiles)
  const changed = new Set(current.filter(([id, revision]) => revisions.get(id) !== revision).map(([id]) => id))
  const retained = new Set(current.map(([id]) => id))
  selected.value = new Set([...selected.value].filter(key => {
    const [id]: [string] = JSON.parse(key)
    return retained.has(id) && !changed.has(id)
  }))
})
watchEffect(() => {
  if (props.pane !== 'buckets') return
  enrich(baseRows.value, requiredFeatures.value, requiredQuota.value)
  enrich([...pageRows.value, ...chosen.value], optionalFeatures.value, optionalQuota.value)
})

function incomplete(row: WorkbenchRow, sections: ReadonlySet<Configuration>): boolean {
  return [...sections].some(section => reading(row, section) === undefined)
}

function choosePane(pane: 'overview' | 'buckets'): void {
  if (pane === 'overview') emit('overview')
  else emit('inventory')
}

function filterContext(id = ''): void {
  context.value = id
  emit('inventory')
}

function getChecked(event: Event): boolean {
  return (event.currentTarget as HTMLInputElement).checked
}

function includeRows(targets: readonly WorkbenchRow[], checked: boolean): void {
  const updated = new Set(selected.value)
  for (const row of targets) {
    if (checked) updated.add(row.key)
    else updated.delete(row.key)
  }
  selected.value = updated
}

function clearFilters(): void {
  query.value = ''
  context.value = ''
  filters.value = []
}

function includeColumn(id: string, checked: boolean): void {
  columns.value = checked ? [...columns.value, id] : columns.value.filter(column => column !== id)
}

function displayCell(column: typeof workbenchColumns[number], row: WorkbenchRow): string {
  const value = rowCell(column, row, reading, tags)
  if (value === null) {
    if (column.section && !reading(row, column.section) || column.quota && quotaPending(row)) return 'Reading…'
    if (column.quota && quotaError(row)) return 'Read error'
    return '—'
  }
  if (column.id === 'bytes') return formatSize(row.bytes)
  if (column.id === 'created') return formatDate(row.createdAt)
  if (typeof value === 'number') return value.toLocaleString()
  if (typeof value === 'boolean') return value ? 'Yes' : 'No'
  return value || '—'
}

function cellError(column: typeof workbenchColumns[number], row: WorkbenchRow): string | undefined {
  if (column.quota) return quotaError(row)
  const feature = column.section && reading(row, column.section)
  return feature && feature.status === 'error' ? feature.message : undefined
}

async function exportSelection(): Promise<void> {
  exporting.value = true
  actionError.value = ''
  try {
    await saveBucketSelection(exportFormat.value, exportWorkbenchRows(chosen.value, visibleColumns.value, exportFormat.value, reading, tags))
  } catch (failure) {
    actionError.value = errorMessage(failure)
  } finally {
    exporting.value = false
  }
}

function classify(row: WorkbenchRow, value: string): void {
  actionError.value = ''
  try {
    saveTags(row, value)
  } catch (failure) {
    actionError.value = errorMessage(failure)
  }
}

function discard(): void {
  details.value?.discard()
}

defineExpose({ dirty, busy: editing, discard })
</script>

<template>
  <section class="workspace-page workbench" :class="{ 'workspace-compact': compact }">
    <WorkspaceHeader :title="source === 'storageOps' ? 'Storage Ops' : 'RGW buckets'"
      :description="source === 'storageOps' ? 'Inventory across registered storage contexts' : 'Bucket inventory from the selected Ceph gateway'">
      <template #actions>
        <button type="button" :disabled="busy || !profiles.length" @click="refresh()"><RefreshCw :size="16" :class="{ spinning: busy }" /> Refresh</button>
      </template>
    </WorkspaceHeader>
    <WorkspaceTabs v-if="source === 'storageOps'" :model-value="pane" :tabs="tabs" label="Storage Ops views" @update:model-value="choosePane" />

    <div v-show="pane === 'overview'" class="workbench-overview">
      <div class="workbench-metrics">
        <WorkspaceMetric title="Registered contexts" :value="profiles.length" detail="Connections with Storage Ops access" @open="filterContext()" />
        <WorkspaceMetric title="Visible bucket rows" :value="rows.length.toLocaleString()" detail="Counted separately for each context" @open="filterContext()" />
      </div>
      <WorkspacePanel title="Storage contexts">
        <div v-if="contexts.length" class="workbench-table">
          <table class="report-table">
            <thead><tr><th scope="col">Context</th><th scope="col">Endpoint</th><th scope="col">Buckets</th><th scope="col">Inventory</th></tr></thead>
            <tbody>
              <tr v-for="{ profile, snapshot } in contexts" :key="profile.id">
                <th scope="row"><button class="subtle bucket-record" type="button" @click="filterContext(profile.id)"><Server :size="16" />{{ profile.name }}</button></th>
                <td>{{ profile.endpoint || providerNames[profile.provider] }}<small class="muted context-meta">{{ profile.settings?.tenant || profile.region }}</small></td>
                <td>{{ snapshot?.rows.length.toLocaleString() ?? '—' }}</td>
                <td>{{ snapshot?.pending ? 'Refreshing…' : snapshot?.inventoryError ? 'Read failed' : formatDate(snapshot?.loadedAt) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <EmptyState v-else :icon="Server" title="No registered storage contexts"><button type="button" @click="emit('connections')">Manage connections</button></EmptyState>
      </WorkspacePanel>
    </div>

    <WorkspacePanel v-show="pane === 'buckets'" class="register-panel" title="Bucket inventory">
      <div class="workbench-controls">
        <input v-model="query" type="search" aria-label="Search bucket names" placeholder="Search bucket names" />
        <AppSelect v-model="match" :options="searchModes" aria-label="Bucket name match" />
        <AppSelect v-if="source === 'storageOps'" v-model="context" :options="contextOptions" aria-label="Filter by context" />
        <button type="button" @click="showFilters = true"><Filter :size="15" /> Filters<span v-if="filters.length" class="chip">{{ filters.length }}</span></button>
        <details class="tool-menu columns-menu">
          <summary><Columns3 :size="16" /> Columns</summary>
          <div class="pop-menu" aria-label="Inventory columns">
            <label v-for="column in workbenchColumns" :key="column.id" class="check-label">
              <input type="checkbox" :checked="columns.includes(column.id)" :disabled="column.id === 'bucket' || column.id === 'context'"
                @change="includeColumn(column.id, getChecked($event))" />{{ column.label }}
            </label>
          </div>
        </details>
      </div>
      <div class="workbench-selection">
        <span role="status">{{ filteredRows.length.toLocaleString() }} buckets · {{ chosen.length.toLocaleString() }} selected<span v-if="coveragePending"> · Reading filtered features…</span></span>
        <div class="panel-actions">
          <button type="button" :disabled="!filteredRows.length || busy || coveragePending" @click="includeRows(filteredRows, true)">Select filtered</button>
          <button type="button" :disabled="!chosen.length" @click="selected = new Set()">Clear selection</button>
          <button type="button" :disabled="!chosen.length || busy" @click="showCampaign = true"><ClipboardList :size="15" /> New campaign</button>
          <button v-if="cephProfile" type="button" :disabled="!indexTargets.length || busy" title="Select 1 to 200 current bucket instances" @click="showIndexBatch = true">Check indexes</button>
          <AppSelect v-model="exportFormat" :options="exportFormats" aria-label="Export format" />
          <button type="button" :disabled="!chosen.length || exportPending || exporting" @click="exportSelection"><Download :size="15" /> Export selection</button>
        </div>
      </div>
      <div v-if="filteredRows.length" class="workbench-table">
        <table class="report-table">
          <thead><tr>
            <th scope="col"><input type="checkbox" aria-label="Select current page" :checked="everyPageRow" :indeterminate.prop="selectedOnPage && !everyPageRow"
              @change="includeRows(pageRows, getChecked($event))" /></th>
            <th v-for="column in visibleColumns" :key="column.id" scope="col">{{ column.label }}</th>
          </tr></thead>
          <tbody>
            <tr v-for="row in pageRows" :key="row.key" :class="{ 'workbench-selected': selected.has(row.key) }">
              <td><input type="checkbox" :aria-label="`Select ${row.name} in ${row.profile.name}, ${row.tenant || 'default tenant'}`"
                :checked="selected.has(row.key)" @change="includeRows([row], getChecked($event))" /></td>
              <th scope="row"><button class="subtle bucket-record" type="button" @click="emit('inspect', row.key)"><Database :size="16" />{{ row.name }}</button></th>
              <td v-for="column in visibleColumns.slice(1)" :key="column.id" :title="cellError(column, row)">
                {{ displayCell(column, row) }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <EmptyState v-else-if="!busy && !coveragePending" :icon="Database" :title="rows.length ? 'No matching buckets' : 'No visible buckets'">
        <button v-if="query || context || filters.length" type="button" @click="clearFilters">Clear filters</button>
        <button v-else-if="!profiles.length" type="button" @click="emit('connections')">Manage connections</button>
      </EmptyState>
      <p v-else class="inventory-loading muted" role="status">Reading bucket inventory…</p>
      <div v-if="filteredRows.length" class="workbench-pagination">
        <button class="icon-button" type="button" aria-label="Previous page" :disabled="page === 1" @click="page--"><ChevronLeft :size="16" /></button>
        <span>Page {{ page }} of {{ pageCount }}</span>
        <button class="icon-button" type="button" aria-label="Next page" :disabled="page >= pageCount" @click="page++"><ChevronRight :size="16" /></button>
      </div>
    </WorkspacePanel>
    <template v-for="{ profile, snapshot } in contexts" :key="profile.id">
      <Notice v-if="snapshot?.inventoryError" tone="error">{{ profile.name }} · {{ snapshot.inventoryError }} <template v-if="snapshot.loadedAt">Showing cached rows.</template></Notice>
      <Notice v-if="snapshot?.usageError">{{ profile.name }} usage · {{ snapshot.usageError }}</Notice>
    </template>
    <Notice v-if="actionError" tone="error">{{ actionError }}</Notice>
    <BucketCampaign v-if="showCampaign" ref="campaign" :rows="rows" :selection="chosen" :clipboard="clipboard"
      @close="showCampaign = false" @clipboard="value => clipboard = value" @result="applyResult" />
    <CephIndexBatch v-if="showIndexBatch && cephProfile" :profile="cephProfile" :targets="indexTargets" @close="showIndexBatch = false" />
    <BucketWorkbenchFilters v-if="showFilters" :filters="filters" @close="showFilters = false" @apply="value => { filters = value; showFilters = false }" />
    <Modal v-if="target" :title="focused?.name ?? 'Bucket details'" drawer @close="emit('inspect')">
      <div class="modal-form">
        <Notice v-if="actionError" tone="error">{{ actionError }}</Notice>
        <FadeRegion>
          <BucketWorkbenchDetail v-if="focused" :key="`${focused.key}:${focused.target.revision}`" ref="details" :row="focused"
            :tags="tags(focused)" :previous="previous?.key" :next="next?.key" :link="link" @inspect="emit('inspect', $event)"
            @classify="classify(focused, $event)" @changed="invalidate(focused)" @inventory-changed="refresh()" />
          <p v-else-if="busy" role="status">Finding the bucket in its registered context…</p>
          <Notice v-else>The bucket is unavailable in this context. Refresh the inventory to check again.</Notice>
        </FadeRegion>
      </div>
    </Modal>
  </section>
</template>

<style scoped>
.workbench, .workbench-overview { display: grid; gap: 14px; }
.workbench .workspace-heading { margin-bottom: 0; }
.register-panel { overflow: visible; }
.columns-menu .pop-menu { right: 0; }
.workbench-metrics { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.workbench-table { overflow: auto; }
.workbench-table .report-table td { max-width: 310px; overflow-wrap: anywhere; }
.workbench-table .report-table th:first-child, .workbench-table .report-table td:first-child { width: 1%; }
.workbench-table .report-table th { white-space: nowrap; }
.workbench-table .workbench-selected td, .workbench-table .workbench-selected th { background: var(--selected); }
.context-meta { display: block; }
.workbench-controls, .workbench-selection, .workbench-pagination {
  display: flex; align-items: center; flex-wrap: wrap; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--line);
}
.workbench-controls > input { flex: 1 1 180px; }
.workbench-selection { justify-content: space-between; color: var(--muted); }
.workbench-pagination { justify-content: flex-end; border-bottom: 0; color: var(--muted); }
.columns-menu .pop-menu { width: max-content; max-height: 290px; overflow: auto; padding: 9px 11px; }
.columns-menu { z-index: 5; }
.inventory-loading { padding: 12px 14px; }
@media (max-width: 650px) {
  .workbench-metrics { grid-template-columns: 1fr; }
  .workbench-controls, .workbench-selection { align-items: stretch; }
}
</style>
