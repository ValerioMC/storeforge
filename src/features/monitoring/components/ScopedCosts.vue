<script setup lang="ts">
import { computed } from 'vue'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import { formatCost, measureSubject, subjectLabel, type ScopedCostSummary } from '../model/storage-costs'
import CostBreakdown from './CostBreakdown.vue'
import '../storage-costs.css'

const props = defineProps<{ summary?: ScopedCostSummary | null; pending?: boolean; error?: string; month: string }>()
const row = computed(() => props.summary && measureSubject(props.summary.measurements, props.month, '', props.summary.rates))
</script>

<template>
  <WorkspacePanel title="Estimated storage costs" :description="`${month} UTC`">
    <template v-if="row && summary">
      <p class="cost-summary-line"><strong>{{ formatCost(row.total, summary.currency) }}</strong> {{ subjectLabel(row.subject) }} · {{ row.storageDays }}/{{ row.daysInMonth }} storage days · {{ row.trafficDays }}/{{ row.daysInMonth }} traffic days</p>
      <CostBreakdown :totals="row" :currency="summary.currency" />
      <p v-if="!summary.rates" class="workspace-panel-message">Pricing is unavailable for this endpoint.</p>
    </template>
    <p v-else class="workspace-panel-message" :role="error ? 'alert' : 'status'">{{ error || (pending ? 'Reading cost measurements…' : 'No cost measurements for this connection’s managed RGW account or user.') }}</p>
  </WorkspacePanel>
</template>
