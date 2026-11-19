<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { formatDate, formatSize } from '../../../common/format'
import { formatCount } from '../../monitoring/model/dashboard'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import { portalApi } from '../api/portal'
import { governanceApi } from '../api/governance'
import { usePortalTask } from '../composables/usePortalTask'
import { localDate, type PortalComposition, type PortalCost, type PortalTraffic, type TrafficWindow } from '../model/governance'
import type { PortalProject, PortalScope, PortalState, PortalUsage } from '../model/spaces'
import PortalDashboard from './PortalDashboard.vue'

const props = defineProps<{ scope: PortalScope; project: PortalProject }>()
const emit = defineEmits<{ space: [id: string]; spaces: [] }>()
const state = usePortalTask<PortalState>()
const usage = usePortalTask<PortalUsage>()
const composition = usePortalTask<PortalComposition>()
const traffic = usePortalTask<PortalTraffic>()
const costs = usePortalTask<PortalCost>()
const { data: trafficData } = traffic
const { data: cost } = costs
const { data: usageData } = usage
const tab = ref<'overview' | 'spaces' | 'traffic' | 'composition' | 'cost'>('overview')
const window = ref<TrafficWindow>('week')
const month = ref(localDate().slice(0, 7))
const tabs = computed(() => [
  { value: 'overview' as const, label: 'Overview' }, { value: 'spaces' as const, label: 'By space' },
  { value: 'traffic' as const, label: 'Traffic' },
  ...(state.data.value?.bucket_usage_stats_enabled ? [{ value: 'composition' as const, label: 'Composition' }] : []),
  ...(state.data.value?.cost_estimation_enabled ? [{ value: 'cost' as const, label: 'Cost' }] : []),
])
const windows = [{ value: 'hour', label: 'Hour' }, { value: 'day', label: 'Day' }, { value: 'week', label: 'Week' }, { value: 'month', label: 'Month' }] as const
const categories = [
  { value: 'data_type_distribution', label: 'File types' }, { value: 'storage_class_distribution', label: 'Storage classes' },
  { value: 'size_distribution', label: 'File sizes' }, { value: 'age_distribution', label: 'Object age' },
  { value: 'current_vs_noncurrent', label: 'Current and historical versions' },
] as const
const category = ref<typeof categories[number]['value']>('data_type_distribution')
const rows = computed(() => composition.data.value?.aggregate[category.value] ?? [])
const active = computed(() => tab.value === 'spaces' ? usage : tab.value === 'traffic' ? traffic : tab.value === 'composition' ? composition : tab.value === 'cost' ? costs : undefined)
const maximumTraffic = computed(() => Math.max(1, ...trafficData.value?.series.flatMap(point => [point.bytes_in, point.bytes_out]) ?? []))

onMounted(() => void state.run(() => portalApi.state(props.scope)))
watch([tab, window, month], ([current]) => {
  if (current === 'spaces') void usage.run(() => portalApi.usage(props.scope))
  if (current === 'traffic') void traffic.run(() => governanceApi.traffic(props.scope, window.value))
  if (current === 'composition') void composition.run(() => governanceApi.composition(props.scope))
  if (current === 'cost' && /^\d{4}-(0[1-9]|1[0-2])$/.test(month.value)) void costs.run(() => governanceApi.cost(props.scope, month.value))
})

function price(value?: number | null): string {
  const currency = cost.value?.cost?.currency
  return value == null || !currency ? '—' : new Intl.NumberFormat(undefined, { style: 'currency', currency }).format(value)
}
</script>

<template>
  <Notice v-if="state.error.value" tone="error">{{ state.error.value }}</Notice>
  <WorkspaceTabs v-model="tab" label="Storage health views" :tabs="tabs" />
  <PortalDashboard v-if="tab === 'overview'" :scope="scope" :project="project" @open="emit('space', $event)" @spaces="emit('spaces')" />
  <section v-else class="portal-panel">
    <div class="portal-between">
      <h2>{{ tabs.find(item => item.value === tab)?.label }}</h2>
      <AppSelect v-if="tab === 'traffic'" v-model="window" :options="windows" aria-label="Traffic period" />
      <AppSelect v-if="tab === 'composition'" v-model="category" :options="categories" aria-label="Storage composition" />
      <label v-if="tab === 'cost'" class="portal-inline">Month <input v-model="month" type="month" /></label>
    </div>
    <Notice v-if="active?.error.value" tone="error">{{ active.error.value }}</Notice>
    <p v-if="active?.pending.value" class="muted portal-note" role="status">Loading storage measurements…</p>
    <div v-if="tab === 'spaces' && usageData" class="portal-table-scroll">
      <table class="portal-table">
        <thead><tr><th scope="col">Space</th><th scope="col">Used storage</th><th scope="col">Storage quota</th><th scope="col">Objects</th></tr></thead>
        <tbody>
          <tr v-for="space in usageData.storage_spaces" :key="space.id">
            <th scope="row"><button type="button" class="subtle portal-button-label" @click="emit('space', space.id)">{{ space.name }}</button></th>
            <td>{{ formatSize(space.used_bytes) }}</td><td>{{ formatSize(space.quota_max_size_bytes) }}</td><td>{{ formatCount(space.object_count) }}</td>
          </tr>
          <tr v-if="usageData.other_storage_space">
            <th scope="row">Other usage</th><td>{{ formatSize(usageData.other_storage_space.used_bytes) }}</td><td>—</td><td>{{ formatCount(usageData.other_storage_space.object_count) }}</td>
          </tr>
          <tr v-if="!usageData.storage_spaces.length && !usageData.other_storage_space"><td colspan="4" class="muted">No storage usage measurements.</td></tr>
        </tbody>
      </table>
    </div>
    <template v-if="tab === 'traffic' && trafficData">
      <p class="muted portal-note">{{ formatDate(trafficData.start, true) }} to {{ formatDate(trafficData.end, true) }}</p>
      <dl class="portal-statistics">
        <div><dt>Uploaded</dt><dd>{{ formatSize(trafficData.totals.bytes_in) }}</dd></div>
        <div><dt>Downloaded</dt><dd>{{ formatSize(trafficData.totals.bytes_out) }}</dd></div>
        <div><dt>Operations</dt><dd>{{ formatCount(trafficData.totals.ops) }}</dd></div>
        <div><dt>Successful</dt><dd>{{ formatCount(trafficData.totals.success_ops) }}</dd></div>
      </dl>
      <div v-if="trafficData.series.length" class="portal-traffic" aria-label="Upload and download traffic">
        <div v-for="point in trafficData.series" :key="point.timestamp" class="traffic-point"
          :title="`${formatDate(point.timestamp, true)} · Uploaded ${formatSize(point.bytes_in)} · Downloaded ${formatSize(point.bytes_out)}`">
          <span :style="{ height: `${point.bytes_in / maximumTraffic * 100}%` }" />
          <span :style="{ height: `${point.bytes_out / maximumTraffic * 100}%` }" />
        </div>
      </div>
      <p v-if="trafficData.series.length" class="muted portal-note">Blue bars show uploads. Green bars show downloads.</p>
    </template>
    <template v-if="tab === 'composition' && composition.data.value">
      <p class="muted portal-note">Snapshots cover {{ composition.data.value.aggregate.buckets_with_snapshot }} of {{ composition.data.value.aggregate.bucket_count }} spaces.</p>
      <div class="portal-table-scroll">
        <table class="portal-table">
          <thead><tr><th scope="col">Group</th><th scope="col">Objects</th><th scope="col">Size</th></tr></thead>
          <tbody>
            <tr v-for="row in rows" :key="row.key"><th scope="row">{{ row.label }}</th><td>{{ formatCount(row.count) }}</td><td>{{ formatSize(row.bytes) }}</td></tr>
            <tr v-if="!rows.length"><td colspan="3" class="muted">No measurements for this grouping.</td></tr>
          </tbody>
        </table>
      </div>
    </template>
    <template v-if="tab === 'cost' && cost">
      <p class="muted portal-note">{{ cost.coverage.days_collected }} of {{ cost.coverage.days_in_month }} days collected</p>
      <dl class="portal-statistics">
        <div><dt>Total estimate</dt><dd>{{ price(cost.cost?.total_cost) }}</dd></div><div><dt>Storage</dt><dd>{{ price(cost.cost?.storage_cost) }}</dd></div>
        <div><dt>Egress</dt><dd>{{ price(cost.cost?.egress_cost) }}</dd></div><div><dt>Ingress</dt><dd>{{ price(cost.cost?.ingress_cost) }}</dd></div>
        <div><dt>Requests</dt><dd>{{ price(cost.cost?.requests_cost) }}</dd></div>
      </dl>
    </template>
  </section>
</template>

<style scoped>
.portal-traffic { display: flex; align-items: stretch; gap: 3px; height: 104px; border-bottom: 1px solid var(--line); }
.traffic-point { display: flex; align-items: end; gap: 1px; flex: 1; }
.traffic-point span { flex: 1; background: var(--accent); }
.traffic-point span + span { background: var(--success); }
</style>
