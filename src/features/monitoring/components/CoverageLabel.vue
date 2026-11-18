<script setup lang="ts">
import { coverageLabel, type Coverage } from '../model/monitoring'

defineProps<{ coverage?: Coverage; stale?: boolean; detail?: string | null }>()
</script>

<template>
  <span class="monitoring-coverage" :title="detail || undefined" :data-state="coverage ?? 'missing'">
    {{ coverageLabel[coverage ?? 'missing'] }}<template v-if="stale"> · Stale</template>
  </span>
</template>
