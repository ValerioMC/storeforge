<script setup lang="ts">
import { Columns3 } from 'lucide-vue-next'
import type { DetailColumn } from '../model/listing'
import type { BrowserStore } from '../store/browser'

defineProps<{ browser: BrowserStore }>()
const detailColumns: { id: DetailColumn; label: string }[] = [
  { id: 'contentType', label: 'Content type' }, { id: 'versionId', label: 'Version ID' }, { id: 'etag', label: 'ETag' },
]
</script>

<template>
  <details class="listing-columns tool-menu">
    <summary><Columns3 :size="15" /> Columns</summary>
    <div class="pop-menu listing-column-menu">
      <label><input v-model="browser.technical" type="checkbox" /> Storage class</label>
      <label v-for="column in detailColumns" :key="column.id"><input v-model="browser.detailColumns" type="checkbox" :value="column.id" /> {{ column.label }}</label>
      <button @click="browser.columnWidths = {}">Reset widths</button>
    </div>
  </details>
</template>

<style scoped>
.listing-column-menu { left: 0; right: auto; padding: 9px; gap: 6px; }
.listing-column-menu label { display: flex; align-items: center; gap: 6px; }
</style>
