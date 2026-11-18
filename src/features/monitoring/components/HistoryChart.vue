<script setup lang="ts">
import { computed } from 'vue'
import { formatDate, formatSize } from '../../../common/format'
import type { HistorySlot } from '../model/monitoring'

const props = defineProps<{ slots: HistorySlot[]; hourly?: boolean }>()
const ordered = computed(() => [...props.slots].reverse())
const values = computed(() => ordered.value.flatMap(slot => slot.sample?.currentBytes == null ? [] : [slot.sample.currentBytes]))
const scale = computed(() => Math.max(...values.value, 1))
</script>

<template>
  <figure v-if="values.length" class="monitoring-chart" aria-label="Current object storage history">
    <div class="monitoring-axis" aria-hidden="true">
      <span>{{ formatSize(values.length && scale === 1 ? Math.max(...values) : scale) }}</span><span>0 B</span>
    </div>
    <div class="monitoring-bars" aria-hidden="true">
      <span v-for="slot in ordered" :key="slot.periodStart" class="monitoring-bar-slot"
        :title="`${formatDate(slot.periodStart, hourly)} · ${slot.sample?.currentBytes == null ? 'No measurement' : formatSize(slot.sample.currentBytes)}`">
        <span v-if="slot.sample?.currentBytes != null" class="monitoring-bar" :data-partial="slot.sample.coverage === 'partial'"
          :style="{ '--height': `${(slot.sample.currentBytes / scale) * 100}%` }" />
      </span>
    </div>
    <figcaption>
      <span>{{ formatDate(ordered[0]?.periodStart, hourly) }}</span>
      <span>Empty intervals indicate gaps</span>
      <span>{{ formatDate(ordered.at(-1)?.periodStart, hourly) }}</span>
    </figcaption>
  </figure>
  <p v-else class="workspace-panel-message">No storage samples in this period.</p>
</template>
