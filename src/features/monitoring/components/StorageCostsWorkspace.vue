<script setup lang="ts">
import { computed, nextTick, onActivated, ref, watch } from 'vue'
import { ArrowDown, ArrowLeft, ArrowUp, ArrowUpRight, Database, Download } from 'lucide-vue-next'
import { formatSize } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import QueryPager from '../../administration/components/QueryPager.vue'
import { collectDailyCosts, exportCosts, readCostReport, saveCostSettings } from '../api/storage-costs'
import { formatCount } from '../model/dashboard'
import {
  costCsv, costHistory, costPortfolioRows, costRows, formatCost, measurementKey, portfolioTotals,
  shiftCostMonth, sortCostRows, subjectLabel, topCosts, utcDay, validCostMonth,
  type CostCollection, type CostFilters, type CostReport, type CostRow, type CostSettings, type CostSort,
} from '../model/storage-costs'
import CostBreakdown from './CostBreakdown.vue'
import CostSettingsPanel from './CostSettings.vue'
import CostSubjectDetail from './CostSubjectDetail.vue'
import CostTrend from './CostTrend.vue'
import '../storage-costs.css'

const props = defineProps<{ subject?: string; subjectMonth?: string; compact?: boolean; scopeKey?: string }>()
const emit = defineEmits<{ inspect: [key: string, month: string]; back: [] }>()
const tabs = [
  { value: 'overview', label: 'Overview' },
  { value: 'subjects', label: 'Subjects' },
  { value: 'collection', label: 'Collection' },
  { value: 'settings', label: 'Cost settings' },
] as const
const kindOptions = [
  { value: 'all', label: 'All subjects' },
  { value: 'account', label: 'Accounts' },
  { value: 'user', label: 'Users' },
] as const
const columns = [
  { key: 'name', label: 'Subject' }, { key: 'endpoint', label: 'Endpoint' }, { key: 'total', label: 'Estimate' },
  { key: 'storage', label: 'Storage' }, { key: 'ingress', label: 'Ingress' },
  { key: 'egress', label: 'Egress' }, { key: 'requests', label: 'Requests' },
] as const
const pageSize = 10
const today = utcDay()
const month = ref(today.slice(0, 7))
const monthInput = ref(month.value)
const endpoint = ref('')
const kind = ref<CostFilters['kind']>('all')
const search = ref('')
const day = ref(today)
const collectionEndpoint = ref('')
const tab = ref<(typeof tabs)[number]['value']>('overview')
const page = ref(0)
const sort = ref<CostSort>('total')
const descending = ref(true)
const report = ref<CostReport>()
const loadedMonth = ref('')
const loading = ref(false)
const busy = ref<'collect' | 'save' | 'export'>()
const error = ref('')
const monthError = ref('')
const collection = ref<CostCollection>()
const root = ref<HTMLElement>()
const backButton = ref<HTMLButtonElement>()
let returnFocusKey = ''
let activated = false
let generation = 0

const reportMonth = computed(() => props.subject && props.subjectMonth ? props.subjectMonth : month.value)
const filters = computed<CostFilters>(() => ({ month: month.value, endpoint: endpoint.value, kind: kind.value, search: search.value }))
const options = computed(() => [{ value: '', label: 'All endpoints' }, ...(report.value?.endpoints.map(entry => ({ value: entry.id, label: entry.name })) ?? [])])
const rows = computed(() => report.value ? costRows(report.value, filters.value) : [])
const sorted = computed(() => sortCostRows(rows.value, sort.value, descending.value))
const visible = computed(() => sorted.value.slice(page.value * pageSize, (page.value + 1) * pageSize))
const accounted = computed(() => costPortfolioRows(rows.value))
const totals = computed(() => portfolioTotals(accounted.value))
const previousMonth = computed(() => shiftCostMonth(month.value, -1))
const previous = computed(() => report.value ? costRows(report.value, { ...filters.value, month: previousMonth.value }) : [])
const previousTotal = computed(() => portfolioTotals(costPortfolioRows(previous.value)).total)
const delta = computed(() => totals.value.total != null && previousTotal.value != null ? totals.value.total - previousTotal.value : null)
const ranked = computed(() => topCosts(accounted.value))
const detailFilters = computed<CostFilters>(() => ({ month: reportMonth.value, endpoint: '', kind: 'all', search: '' }))
const series = computed(() => report.value ? costHistory(report.value, props.subject ? detailFilters.value : filters.value, props.subject) : [])
const detail = computed(() => report.value && costRows(report.value, detailFilters.value).find(row => row.key === props.subject))
const previousDetail = computed(() => report.value && costRows(report.value, {
  ...detailFilters.value, month: shiftCostMonth(reportMonth.value, -1),
}).find(row => row.key === props.subject))
const detailMeasurements = computed(() => report.value?.measurements.filter(row =>
  row.day.startsWith(`${reportMonth.value}-`) && measurementKey(row) === props.subject) ?? [])
const currency = computed(() => report.value?.settings.currency ?? 'USD')
const showFilters = computed(() => tab.value === 'overview' || tab.value === 'subjects')
const canCollect = computed(() => !!report.value?.endpoints.length && (report.value.settings.collectStorage || report.value.settings.collectTraffic))
const pageCount = computed(() => Math.max(1, Math.ceil(sorted.value.length / pageSize)))

watch([reportMonth, () => props.scopeKey], ([, scope], [, priorScope]) => {
  if (scope !== priorScope) {
    report.value = undefined
    collection.value = undefined
  }
  void load()
}, { immediate: true })
onActivated(() => {
  if (activated && !loading.value && !busy.value) void load()
  activated = true
})
watch(filters, () => { page.value = 0 })
watch([pageCount, loadedMonth], ([count, loaded]) => {
  if (loaded === month.value) page.value = Math.min(page.value, count - 1)
})
watch(() => props.subject, async subject => {
  await nextTick()
  if (subject) backButton.value?.focus()
  else [...(root.value?.querySelectorAll<HTMLButtonElement>('[data-cost-subject]') ?? [])]
    .find(button => button.dataset.costSubject === returnFocusKey && !button.closest('[inert]'))?.focus()
})

async function load(): Promise<void> {
  const request = ++generation
  const selectedMonth = reportMonth.value
  loading.value = true
  error.value = ''
  try {
    const result = await readCostReport(selectedMonth)
    if (request !== generation) return
    report.value = result
    loadedMonth.value = selectedMonth
    if (endpoint.value && !result.endpoints.some(item => item.id === endpoint.value)) endpoint.value = ''
    if (collectionEndpoint.value && !result.endpoints.some(item => item.id === collectionEndpoint.value)) collectionEndpoint.value = ''
  } catch (failure) {
    if (request === generation) error.value = errorMessage(failure)
  } finally {
    if (request === generation) loading.value = false
  }
}

function changeMonth(event: Event): void {
  if (!(event.target instanceof HTMLInputElement)) return
  monthInput.value = event.target.value
  if (!validCostMonth(monthInput.value)) {
    monthError.value = 'Use YYYY-MM for the UTC month.'
    return
  }
  monthError.value = ''
  month.value = monthInput.value
}

function changeSort(field: CostSort): void {
  descending.value = sort.value === field ? !descending.value : field !== 'name' && field !== 'endpoint'
  sort.value = field
  page.value = 0
}

function inspect(row: CostRow): void {
  returnFocusKey = row.key
  emit('inspect', row.key, month.value)
}

async function runCollection(): Promise<void> {
  busy.value = 'collect'
  collection.value = undefined
  error.value = ''
  try {
    collection.value = await collectDailyCosts(day.value, collectionEndpoint.value)
    await load()
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = undefined
  }
}

async function save(settings: CostSettings): Promise<void> {
  busy.value = 'save'
  error.value = ''
  try {
    await saveCostSettings(settings)
    await load()
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = undefined
  }
}

async function download(): Promise<void> {
  busy.value = 'export'
  error.value = ''
  try {
    await exportCosts(month.value, costCsv(sorted.value, month.value, currency.value))
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = undefined
  }
}

function endpointName(id: string): string {
  return report.value?.endpoints.find(item => item.id === id)?.name ?? id
}
</script>

<template>
  <section ref="root" class="workspace-page cost-workspace" :class="{ 'workspace-compact': compact }">
    <WorkspaceHeader :title="subject && detail ? detail.name : 'Storage costs'"
      :description="subject ? detail?.subject.kind === 'account' ? 'Managed RGW account' : detail ? 'Managed RGW user' : undefined : 'Monthly estimates for managed RGW accounts and users'">
      <template v-if="subject" #actions>
        <button ref="backButton" type="button" @click="emit('back')"><ArrowLeft :size="15" /> Back to costs</button>
      </template>
    </WorkspaceHeader>
    <Notice v-if="error" tone="error">{{ error }} <button v-if="!report || loadedMonth !== reportMonth" type="button" @click="load">Retry</button></Notice>
    <p v-if="loading && loadedMonth !== reportMonth" class="workspace-panel-message" role="status">Reading cost measurements…</p>
    <FadeRegion v-if="subject">
      <div :key="subject">
        <CostSubjectDetail v-if="detail && loadedMonth === reportMonth" :row="detail" :month="reportMonth"
          :previous="previousDetail" :currency="currency" :series="series" :measurements="detailMeasurements" />
        <p v-else-if="!loading" class="workspace-panel-message">This subject has no measurements in the selected month and scope.</p>
      </div>
    </FadeRegion>
    <template v-else>
      <WorkspaceTabs v-model="tab" label="Storage cost sections" :tabs="tabs" />
      <div v-show="showFilters" class="cost-controls cost-filters">
        <label>Month (UTC) <input :value="monthInput" type="month" min="0001-01" :max="today.slice(0, 7)" placeholder="YYYY-MM" :aria-invalid="!!monthError" @change="changeMonth" /></label>
        <label>Endpoint <AppSelect v-model="endpoint" :options="options" aria-label="Cost endpoint" /></label>
        <label>Subject type <AppSelect v-model="kind" :options="kindOptions" aria-label="Subject type" /></label>
        <label>Subject <input v-model="search" type="search" placeholder="Find by name or ID" /></label>
      </div>
      <Notice v-if="monthError && showFilters" tone="error">{{ monthError }}</Notice>
      <template v-if="report && loadedMonth === reportMonth">
        <div class="fade-stack">
          <FadeRegion>
            <CostSettingsPanel v-show="tab === 'settings'" :settings="report.settings" :endpoints="report.endpoints" :busy="!!busy" @save="save" @collection="tab = 'collection'" />
          </FadeRegion>
          <FadeRegion>
            <div v-if="tab !== 'settings'" :key="tab" class="cost-stack cost-tab-content">
              <template v-if="tab === 'overview'">
                <WorkspacePanel title="Estimated cost" :description="!rows.length ? 'No measurements match these filters.' : accounted.some(row => row.partial) ? 'Partial coverage · estimates use available readings' : 'Based on stored daily measurements'">
                  <dl class="cost-breakdown cost-period cost-portfolio-period">
                    <div><dt>{{ month }}</dt><dd>{{ formatCost(totals.total, currency) }}</dd></div>
                    <div><dt>{{ previousMonth }}</dt><dd>{{ formatCost(previousTotal, currency) }}</dd></div>
                    <div><dt>Month over month</dt><dd>{{ delta != null && delta > 0 ? '+' : '' }}{{ formatCost(delta, currency) }}</dd></div>
                  </dl>
                  <CostBreakdown class="cost-secondary-breakdown" :totals="totals" :currency="currency" />
                  <p v-if="rows.length > accounted.length" class="cost-help cost-inset">Portfolio totals count accounts once and show their users separately in Subjects.</p>
                  <p v-if="!report.settings.rates && !Object.keys(report.settings.overrides).length" class="cost-help cost-inset">Set rates in Cost settings to calculate estimates.</p>
                </WorkspacePanel>
                <div class="cost-overview-grid">
                  <CostTrend :series="series" :currency="currency" />
                  <WorkspacePanel title="Top subjects" :description="ranked.fraction == null ? 'Ranked by estimated cost' : `Top 10 share · ${ranked.fraction.toLocaleString(undefined, { style: 'percent', maximumFractionDigits: 1 })}`">
                    <ol v-if="ranked.rows.length" class="cost-rank">
                      <li v-for="row in ranked.rows" :key="row.key">
                        <button type="button" class="subtle" :data-cost-subject="row.key" @click="inspect(row)"><span>{{ row.name }} <small>{{ row.endpoint }}</small></span><strong>{{ formatCost(row.total, currency) }}</strong><ArrowUpRight :size="14" /></button>
                      </li>
                    </ol>
                    <p v-else class="workspace-panel-message">No priced subjects match these filters.</p>
                  </WorkspacePanel>
                </div>
              </template>
              <WorkspacePanel v-else-if="tab === 'subjects'" title="Subjects" :description="`${rows.length} matching ${rows.length === 1 ? 'subject' : 'subjects'}`">
                <template #actions><button type="button" :disabled="!rows.length || !!busy" @click="download"><Download :size="15" /> Export CSV</button></template>
                <p v-if="visible.length" class="cost-table-hint">Scroll across the table for traffic and coverage.</p>
                <div v-if="visible.length" class="workspace-table cost-table cost-subject-table" tabindex="0" role="region" aria-label="Cost subjects">
                  <table>
                    <thead><tr>
                      <th v-for="column in columns" :key="column.key" scope="col"
                        :aria-sort="sort === column.key ? descending ? 'descending' : 'ascending' : undefined">
                        <button type="button" class="subtle" @click="changeSort(column.key)">{{ column.label }}
                          <component :is="descending ? ArrowDown : ArrowUp" v-if="sort === column.key" :size="13" />
                        </button>
                      </th>
                      <th scope="col">Complete days</th>
                    </tr></thead>
                    <tbody>
                      <tr v-for="row in visible" :key="row.key">
                        <th scope="row"><button type="button" class="subtle cost-subject" :data-cost-subject="row.key" @click="inspect(row)"><span>{{ row.name }}<small>{{ row.subject.kind === 'account' ? 'Account' : 'User' }} · {{ subjectLabel(row.subject) }}</small></span><ArrowUpRight :size="14" /></button></th>
                        <td>{{ row.endpoint }}</td>
                        <td>{{ formatCost(row.total, currency) }}</td>
                        <td>{{ formatSize(row.usage.storage) }}</td>
                        <td>{{ formatSize(row.usage.ingress) }}</td>
                        <td>{{ formatSize(row.usage.egress) }}</td>
                        <td>{{ formatCount(row.usage.requests) }}</td>
                        <td>{{ row.storageDays }}/{{ row.daysInMonth }} storage<br />{{ row.trafficDays }}/{{ row.daysInMonth }} traffic</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
                <p v-else class="workspace-panel-message">No cost measurements match these filters.</p>
                <div v-if="pageCount > 1" class="cost-pagination"><span>Page {{ page + 1 }} of {{ pageCount }}</span>
                  <QueryPager :busy="loading" :previous="page > 0" :next="page + 1 < pageCount" @previous="page--" @next="page++" />
                </div>
              </WorkspacePanel>
              <WorkspacePanel v-else title="Collect a UTC day" description="Storage snapshots use today’s UTC date. Traffic can be collected for earlier days.">
                <form class="cost-controls cost-inset" @submit.prevent="runCollection">
                  <label>Collection day <input v-model="day" type="date" :max="today" :disabled="!!busy" required /></label>
                  <label>Endpoint <AppSelect v-model="collectionEndpoint" :options="options" :disabled="!!busy" aria-label="Collection endpoint" /></label>
                  <button class="primary" type="submit" :disabled="!!busy || !canCollect"><Database :size="15" /> {{ busy === 'collect' ? 'Collecting…' : 'Collect daily costs' }}</button>
                </form>
                <p v-if="!report.endpoints.length" class="cost-help cost-inset">Add a Ceph connection with verified Admin Ops access and register its RGW accounts or users.</p>
                <div v-if="collection" class="cost-collection" aria-live="polite">
                  <p>{{ collection.day }} · {{ collection.subjects }} subjects · {{ collection.storageRecords }} storage readings · {{ collection.trafficRecords }} traffic readings</p>
                  <details v-if="collection.failures.length" open>
                    <summary>{{ collection.failures.length }} incomplete or unavailable sources</summary>
                    <ul>
                      <li v-for="(failure, index) in collection.failures" :key="index">
                        <strong>{{ endpointName(failure.endpointId) }}{{ failure.subject ? ` · ${subjectLabel(failure.subject)}` : '' }} · {{ failure.source }}</strong>
                        <span>{{ failure.detail }}</span>
                      </li>
                    </ul>
                  </details>
                </div>
              </WorkspacePanel>
            </div>
          </FadeRegion>
        </div>
      </template>
    </template>
  </section>
</template>
