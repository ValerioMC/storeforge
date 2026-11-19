<script setup lang="ts">
import { computed } from 'vue'
import { formatSize } from '../../../common/format'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import { formatCount } from '../model/dashboard'
import { formatCost, subjectLabel, type CostMeasurement, type CostRow, type CostSeriesPoint } from '../model/storage-costs'
import CostBreakdown from './CostBreakdown.vue'
import CostTrend from './CostTrend.vue'
import CoverageLabel from './CoverageLabel.vue'

const props = defineProps<{
  row: CostRow; month: string; previous?: CostRow; currency: string; series: CostSeriesPoint[]; measurements: CostMeasurement[]
}>()
const daily = computed(() => [...props.measurements].sort((left, right) => right.day.localeCompare(left.day)))
const difference = computed(() => props.row.total != null && props.previous?.total != null ? props.row.total - props.previous.total : null)
</script>

<template>
  <div class="cost-stack">
    <WorkspacePanel title="Monthly estimate" :description="`${month} UTC · ${row.endpoint} · ${subjectLabel(row.subject)}`">
      <dl class="cost-breakdown cost-period">
        <div><dt>Estimate</dt><dd>{{ formatCost(row.total, currency) }}</dd></div>
        <div><dt>Previous month</dt><dd>{{ formatCost(previous?.total, currency) }}</dd></div>
        <div><dt>Change</dt><dd>{{ difference != null && difference > 0 ? '+' : '' }}{{ formatCost(difference, currency) }}</dd></div>
        <div><dt>Complete days</dt><dd>{{ row.storageDays }}/{{ row.daysInMonth }} storage</dd><small>{{ row.trafficDays }}/{{ row.daysInMonth }} traffic</small></div>
      </dl>
      <CostBreakdown class="cost-secondary-breakdown" :totals="row" :currency="currency" />
    </WorkspacePanel>
    <CostTrend :series="series" :currency="currency" />
    <WorkspacePanel title="Daily measurements" description="UTC days with stored readings">
      <div class="workspace-table cost-table cost-daily-table" tabindex="0" role="region" aria-label="Daily measurements">
        <table>
          <thead><tr><th scope="col">Day</th><th scope="col">Storage</th><th scope="col">Ingress</th><th scope="col">Egress</th><th scope="col">Requests</th><th scope="col">Traffic coverage</th></tr></thead>
          <tbody>
            <tr v-for="sample in daily" :key="sample.day">
              <th scope="row">{{ sample.day }}</th>
              <td><div class="cost-reading">{{ formatSize(sample.storage.value) }}<CoverageLabel :coverage="sample.storage.coverage" :detail="sample.storage.detail" /></div></td>
              <td>{{ formatSize(sample.traffic.value?.bytesReceived) }}</td>
              <td>{{ formatSize(sample.traffic.value?.bytesSent) }}</td>
              <td>{{ formatCount(sample.traffic.value?.requests) }}</td>
              <td><CoverageLabel :coverage="sample.traffic.coverage" :detail="sample.traffic.detail" /></td>
            </tr>
          </tbody>
        </table>
      </div>
    </WorkspacePanel>
  </div>
</template>
