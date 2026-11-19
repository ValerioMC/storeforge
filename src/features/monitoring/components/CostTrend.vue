<script setup lang="ts">
import { computed } from 'vue'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import { formatCost, type CostSeriesPoint } from '../model/storage-costs'

const props = defineProps<{ series: CostSeriesPoint[]; currency: string }>()
const ceiling = computed(() => Math.max(0, ...props.series.map(point => point.total ?? 0)))
const hasEstimates = computed(() => props.series.some(point => point.total != null))
const monthLabel = new Intl.DateTimeFormat(undefined, { month: 'short', timeZone: 'UTC' })
</script>

<template>
  <WorkspacePanel title="Monthly trend" description="12 UTC months · gaps show missing estimates, stripes show partial coverage">
    <figure v-if="hasEstimates" class="cost-trend">
      <div class="cost-axis" aria-hidden="true"><span>{{ formatCost(ceiling, currency) }}</span><span>{{ formatCost(0, currency) }}</span></div>
      <ol class="cost-bars" aria-label="Monthly estimates">
        <li v-for="point in series" :key="point.month"
          :title="`${point.month} · ${point.total == null ? 'No estimate' : formatCost(point.total, currency)}${point.partial ? ' · Partial coverage' : ''}`">
          <span class="cost-bar-slot" aria-hidden="true">
            <span v-if="point.total != null" class="cost-bar" :class="{ partial: point.partial, zero: point.total === 0 }"
              :style="{ height: `${ceiling ? point.total / ceiling * 100 : 0}%` }" />
            <span v-else class="cost-bar-missing" />
          </span>
          <span aria-hidden="true">{{ monthLabel.format(new Date(`${point.month}-01T00:00:00Z`)) }}</span>
          <span class="sr-only">{{ point.month }} · {{ point.total == null ? 'No estimate' : formatCost(point.total, currency) }}{{ point.partial ? ' · Partial coverage' : '' }}</span>
        </li>
      </ol>
    </figure>
    <p v-else class="workspace-panel-message">No priced measurements during this period.</p>
  </WorkspacePanel>
</template>
