<script setup lang="ts">
import { computed } from 'vue'
import { ArrowUpRight } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import WorkspaceMetric from '../../../components/shell/WorkspaceMetric.vue'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import type { Connection } from '../../connection/model/connection'
import type { MetricPage } from '../../connection/model/workspace'
import { formatCount, formatFraction, rankedBuckets, storageTotals, storageTrend, transferBytes } from '../model/dashboard'
import { coverageLabel, type MonitoringView } from '../model/monitoring'
import BucketMetricsTable from './BucketMetricsTable.vue'
import HealthPanel from './HealthPanel.vue'
import HistoryChart from './HistoryChart.vue'

const props = defineProps<{ profile: Connection; view?: MonitoringView }>()
defineEmits<{ metrics: [metric: MetricPage]; inventory: []; bucket: [name: string]; health: [] }>()

const sample = computed(() => props.view?.usage ?? undefined)
const totals = computed(() => storageTotals(sample.value))
const history = computed(() => props.view?.daily.slice(0, 31) ?? [])
const trend = computed(() => storageTrend(history.value, sample.value?.bucketCoverage.coverage === 'complete' ? sample.value.buckets : []))
const buckets = computed(() => rankedBuckets(sample.value?.buckets ?? []))
const traffic = computed(() => sample.value?.scopedTraffic)
</script>

<template>
  <div class="workspace-metrics monitoring-kpis">
    <WorkspaceMetric title="Storage used" :value="formatSize(totals.bytes)" :detail="coverageLabel[totals.coverage]" @open="$emit('metrics', 'usage')" />
    <WorkspaceMetric title="Visible buckets" :value="formatCount(sample?.bucketCoverage.value)" :detail="coverageLabel[sample?.bucketCoverage.coverage ?? 'missing']" @open="$emit('inventory')" />
    <WorkspaceMetric title="Current objects" :value="formatCount(totals.objects)" :detail="coverageLabel[totals.coverage]" @open="$emit('metrics', 'usage')" />
    <WorkspaceMetric title="Transfer · 24h" :value="formatSize(transferBytes(traffic))" :detail="coverageLabel[traffic?.coverage ?? 'missing']" @open="$emit('metrics', 'traffic')" />
  </div>

  <div class="monitoring-grid monitoring-storage-grid">
    <WorkspacePanel title="Storage history" description="Current object bytes across collected days">
      <template #actions><button class="subtle" @click="$emit('metrics', 'history')">Usage history <ArrowUpRight :size="14" /></button></template>
      <HistoryChart :slots="history" />
      <dl class="monitoring-facts monitoring-trend">
        <div><dt>Growth</dt><dd>{{ trend.growth == null ? '—' : `${trend.growth > 0 ? '+' : trend.growth < 0 ? '−' : ''}${formatSize(Math.abs(trend.growth))}` }}</dd>
          <small v-if="trend.days != null">Over {{ formatCount(Math.max(1, Math.round(trend.days))) }} days</small></div>
        <div><dt>Projected capacity</dt><dd>{{ trend.projectedDays == null ? '—' : trend.projectedDays === 0 ? 'At limit' : `${formatCount(trend.projectedDays)} days` }}</dd>
          <small>{{ trend.projectionDetail }}</small></div>
        <div><dt>Quota utilization</dt><dd>{{ formatFraction(trend.quotaUsage) }}</dd>
          <small v-if="trend.quotaAt">Measured {{ formatDate(trend.quotaAt) }}</small>
          <small v-if="trend.quotaChange != null">{{ trend.quotaChange.toLocaleString(undefined, { maximumFractionDigits: 1, signDisplay: 'exceptZero' }) }} percentage points</small></div>
      </dl>
    </WorkspacePanel>
    <WorkspacePanel title="Largest buckets" description="Ranked by current logical bytes">
      <template #actions><button class="subtle" @click="$emit('inventory')">View buckets <ArrowUpRight :size="14" /></button></template>
      <BucketMetricsTable :buckets="buckets.slice(0, 5)" @bucket="$emit('bucket', $event)" />
    </WorkspacePanel>
  </div>

  <WorkspacePanel title="Bucket quotas" description="Limits and usage reported by RGW">
    <BucketMetricsTable :buckets="buckets" quotas @bucket="$emit('bucket', $event)" />
  </WorkspacePanel>

  <div class="monitoring-grid">
    <HealthPanel :endpoints="[{ profile, view }]" @open="$emit('health')" />
  </div>
</template>
