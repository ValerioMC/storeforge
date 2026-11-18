<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatDate, formatSize } from '../../../common/format'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import type { MetricPage } from '../../connection/model/workspace'
import { formatCount, formatFraction, mergeDistribution, rankedBuckets, storageTotals, sumKnown, transferBytes } from '../model/dashboard'
import type { MonitoringView } from '../model/monitoring'
import BucketMetricsTable from './BucketMetricsTable.vue'
import CoverageLabel from './CoverageLabel.vue'
import HistoryChart from './HistoryChart.vue'

const props = defineProps<{ view?: MonitoringView; endpointTraffic: boolean }>()
const metric = defineModel<MetricPage>({ required: true })
defineEmits<{ bucket: [name: string] }>()
const tabs = [
  { value: 'usage', label: 'Storage' }, { value: 'history', label: 'Usage history' },
  { value: 'traffic', label: 'Traffic' }, { value: 'cost', label: 'Cost' },
] as const
const period = ref<'hourly' | 'daily'>('daily')
const periods = [{ value: 'hourly', label: 'Hourly' }, { value: 'daily', label: 'Daily' }] as const
const distribution = ref<'sizes' | 'ages' | 'types' | 'storageClasses'>('types')
const distributions = [
  { value: 'types', label: 'Data type' }, { value: 'sizes', label: 'Object size' },
  { value: 'ages', label: 'Object age' }, { value: 'storageClasses', label: 'Storage class' },
] as const
const sample = computed(() => props.view?.usage ?? undefined)
const traffic = computed(() => props.endpointTraffic ? sample.value?.traffic : sample.value?.scopedTraffic)
const totals = computed(() => storageTotals(sample.value))
const buckets = computed(() => rankedBuckets(sample.value?.buckets ?? []))
const rgwStorage = computed(() => {
  const readings = buckets.value.map(bucket => bucket.rgwStorage)
  const measured = readings.filter(reading => reading.value != null)
  const coverage = measured.length
    ? (measured.length === readings.length && sample.value?.bucketCoverage.coverage === 'complete' ? 'complete' : 'partial')
    : readings[0]?.coverage ?? 'missing'
  return { coverage, measured: measured.length, bytes: sumKnown(measured.map(item => item.value?.bytes)), objects: sumKnown(measured.map(item => item.value?.objects)) } as const
})
const bins = computed(() => Object.entries(mergeDistribution(buckets.value, distribution.value)).sort((left, right) => (right[1].bytes ?? -1) - (left[1].bytes ?? -1)))
const maxBin = computed(() => Math.max(1, ...bins.value.map(([, value]) => value.bytes ?? 0)))
const history = computed(() => props.view?.[period.value] ?? [])
</script>

<template>
  <WorkspaceTabs v-model="metric" label="Monitoring metrics" :tabs="tabs" />
  <FadeRegion>
    <div :key="metric" class="monitoring-stack">
      <template v-if="metric === 'usage'">
        <WorkspacePanel v-if="endpointTraffic" title="RGW storage" description="Ceph Admin readings for the selected scope">
          <template #actions><CoverageLabel :coverage="rgwStorage.coverage" :stale="view?.stale" /></template>
          <dl class="monitoring-facts">
            <div><dt>Stored bytes</dt><dd>{{ formatSize(rgwStorage.bytes) }}</dd></div>
            <div><dt>Objects</dt><dd>{{ formatCount(rgwStorage.objects) }}</dd></div>
            <div><dt>Visible buckets measured</dt><dd>{{ rgwStorage.measured }} / {{ buckets.length }}</dd></div>
          </dl>
        </WorkspacePanel>
        <WorkspacePanel title="Object storage" description="Logical bytes and objects from S3 listings">
          <template #actions><CoverageLabel :coverage="totals.coverage" :stale="view?.stale" :detail="sample?.bucketCoverage.detail" /></template>
          <dl class="monitoring-facts">
            <div><dt>Current bytes</dt><dd>{{ formatSize(totals.bytes) }}</dd></div>
            <div><dt>Noncurrent bytes</dt><dd>{{ formatSize(totals.noncurrentBytes) }}</dd></div>
            <div><dt>Current objects</dt><dd>{{ formatCount(totals.objects) }}</dd></div>
            <div><dt>Noncurrent objects</dt><dd>{{ formatCount(totals.noncurrentObjects) }}</dd></div>
            <div><dt>Delete markers</dt><dd>{{ formatCount(totals.deleteMarkers) }}</dd></div>
          </dl>
        </WorkspacePanel>

        <div class="monitoring-grid monitoring-storage-grid">
          <WorkspacePanel title="Buckets by storage" description="Open a row to inspect that bucket">
            <BucketMetricsTable :buckets="buckets" @bucket="$emit('bucket', $event)" />
          </WorkspacePanel>
          <WorkspacePanel title="Object distribution" description="Measured object versions, excluding delete markers">
            <template #actions><AppSelect v-model="distribution" :options="distributions" aria-label="Distribution" /></template>
            <ul v-if="bins.length" class="monitoring-list">
              <li v-for="[label, bin] in bins" :key="label">
                <div><strong>{{ label }}</strong><small>{{ formatCount(bin.objects) }} objects</small></div>
                <div class="monitoring-distribution-value"><strong>{{ formatSize(bin.bytes) }}</strong>
                  <meter v-if="bin.bytes != null" :value="bin.bytes" :max="maxBin" :aria-label="`${label}, ${formatSize(bin.bytes)}`" />
                </div>
              </li>
            </ul>
            <p v-else class="workspace-panel-message">No object distribution readings.</p>
          </WorkspacePanel>
        </div>
        <WorkspacePanel title="Bucket quotas" description="Limits and usage reported by RGW">
          <BucketMetricsTable :buckets="buckets" quotas @bucket="$emit('bucket', $event)" />
        </WorkspacePanel>
      </template>

      <WorkspacePanel v-else-if="metric === 'history'" title="Usage history"
        :description="period === 'hourly' ? 'Last 48 hourly slots' : 'Last 90 daily slots'">
        <template #actions><AppSelect v-model="period" :options="periods" aria-label="History interval" /></template>
        <HistoryChart :slots="history" :hourly="period === 'hourly'" />
        <div v-if="history.some(slot => slot.sample)" class="workspace-table monitoring-table">
          <table>
            <thead><tr><th scope="col">Period</th><th scope="col">Collected</th><th scope="col">Current bytes</th><th scope="col">Current objects</th><th scope="col">Quota used</th><th scope="col">Coverage</th></tr></thead>
            <tbody>
              <tr v-for="slot in history" :key="slot.periodStart">
                <th scope="row">{{ formatDate(slot.periodStart, period === 'hourly') }}</th>
                <td>{{ formatDate(slot.sample?.collectedAt, true) }}</td><td>{{ formatSize(slot.sample?.currentBytes) }}</td>
                <td>{{ formatCount(slot.sample?.currentObjects) }}</td><td>{{ formatFraction(slot.sample?.quotaUsage) }}</td><td><CoverageLabel :coverage="slot.sample?.coverage" /></td>
              </tr>
            </tbody>
          </table>
        </div>
      </WorkspacePanel>

      <WorkspacePanel v-else-if="metric === 'traffic'" title="RGW traffic"
        :description="endpointTraffic ? 'Endpoint-wide readings from the Supervision identity' : 'RGW usage for the account or user ID on this profile'">
        <template #actions><CoverageLabel :coverage="traffic?.coverage" :stale="view?.stale" :detail="traffic?.detail" /></template>
        <p v-if="traffic?.value" class="monitoring-panel-note">{{ formatDate(traffic.value.start, true) }} to {{ formatDate(traffic.value.end, true) }}</p>
        <dl class="monitoring-facts">
          <div><dt>Bytes sent</dt><dd>{{ formatSize(traffic?.value?.totals.bytesSent) }}</dd></div>
          <div><dt>Bytes received</dt><dd>{{ formatSize(traffic?.value?.totals.bytesReceived) }}</dd></div>
          <div><dt>Total requests</dt><dd>{{ formatCount(traffic?.value?.totals.requests) }}</dd></div>
          <div><dt>Successful requests</dt><dd>{{ formatCount(traffic?.value?.totals.successfulRequests) }}</dd></div>
        </dl>
        <p v-if="traffic?.detail" class="monitoring-panel-note">{{ traffic.detail }}</p>
      </WorkspacePanel>

      <WorkspacePanel v-else title="Cost inputs" description="Measured storage and transfer for this scope">
        <dl class="monitoring-facts">
          <div><dt>Current storage</dt><dd>{{ formatSize(totals.bytes) }}</dd></div>
          <div><dt>Noncurrent storage</dt><dd>{{ formatSize(totals.noncurrentBytes) }}</dd></div>
          <div><dt>Transfer · 24h</dt><dd>{{ formatSize(transferBytes(traffic)) }}</dd></div>
        </dl>
        <p class="monitoring-panel-note">Cost estimates require configured rates for this endpoint.</p>
      </WorkspacePanel>
    </div>
  </FadeRegion>
</template>
