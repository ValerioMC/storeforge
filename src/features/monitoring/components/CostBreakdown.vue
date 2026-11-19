<script setup lang="ts">
import { formatSize } from '../../../common/format'
import { costComponents, formatCost, type CostTotals } from '../model/storage-costs'
import { formatCount } from '../model/dashboard'

defineProps<{ totals: CostTotals; currency: string }>()
</script>

<template>
  <dl class="cost-breakdown">
    <div v-for="component in costComponents" :key="component.key">
      <dt>{{ component.label }}</dt>
      <dd>{{ formatCost(totals.breakdown[component.key], currency) }}</dd>
      <small>{{ component.key === 'requests' ? formatCount(totals.usage.requests) : formatSize(totals.usage[component.key]) }}{{ component.key === 'storage' && totals.usage.storage != null ? ' daily average' : '' }}</small>
    </div>
  </dl>
</template>
