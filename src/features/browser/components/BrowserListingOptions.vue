<script setup lang="ts">
import { computed } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import { defaultOptions, type DetailColumn, type SortColumn } from '../model/listing'
import type { BrowserStore } from '../store/browser'

const props = defineProps<{ browser: BrowserStore }>()
const emit = defineEmits<{ apply: []; reset: [] }>()
const scopes = [{ value: 'path', label: 'Current path' }, { value: 'bucket', label: 'Whole bucket' }] as const
const objectTypes = [{ value: 'all', label: 'All entries' }, { value: 'object', label: 'Objects' }, { value: 'folder', label: 'Folders' }] as const
const storageClasses = computed(() => [
  { value: '', label: 'All storage classes' },
  ...Array.from(new Set(['STANDARD', 'STANDARD_IA', 'ONEZONE_IA', 'INTELLIGENT_TIERING', 'GLACIER', 'GLACIER_IR', 'DEEP_ARCHIVE', 'REDUCED_REDUNDANCY', 'EXPRESS_ONEZONE',
    ...props.browser.rows.flatMap(row => row.storageClass ? [row.storageClass] : []),
  ])).map(value => ({ value, label: value })),
])
const sorts: { value: SortColumn; label: string }[] = [
  { value: 'provider', label: 'Provider order' }, { value: 'name', label: 'Name' }, { value: 'size', label: 'Size' },
  { value: 'modified', label: 'Modified' }, { value: 'storageClass', label: 'Storage class' },
]
const detailColumns: { id: DetailColumn; label: string }[] = [
  { id: 'contentType', label: 'Content type' }, { id: 'versionId', label: 'Version ID' }, { id: 'etag', label: 'ETag' },
]

function resetSearch(): void {
  props.browser.options = defaultOptions()
  props.browser.sort = { column: 'provider', ascending: true }
  props.browser.recursive = false
  emit('reset')
}
</script>

<template>
  <form v-if="browser.advanced" class="listing-options" aria-label="Advanced listing" @submit.prevent="emit('apply')">
    <div class="listing-fields">
      <div><label for="search-scope">Search scope</label><AppSelect id="search-scope" v-model="browser.options.scope" :options="scopes" /></div>
      <div><label for="search-kind">Entry type</label><AppSelect id="search-kind" v-model="browser.options.kind" :options="objectTypes" /></div>
      <div><label for="search-class">Storage class</label><AppSelect id="search-class" v-model="browser.options.storageClass" :options="storageClasses" /></div>
      <div><label for="search-sort">Sort by</label><AppSelect id="search-sort" v-model="browser.sort.column" :options="sorts" /></div>
      <label>Result limit<input v-model.number="browser.options.limit" type="number" :min="1" :max="5000" required /></label>
    </div>
    <div class="listing-switches">
      <label><input v-model="browser.recursive" type="checkbox" /> Recursive</label>
      <label><input v-model="browser.options.exact" type="checkbox" /> Exact name</label>
      <label><input v-model="browser.options.caseSensitive" type="checkbox" /> Match case</label>
      <label><input v-model="browser.options.includeDeleted" type="checkbox" /> Show deleted</label>
      <label v-if="browser.sort.column !== 'provider'"><input v-model="browser.sort.ascending" type="checkbox" /> Ascending</label>
    </div>
    <div class="listing-actions"><button type="submit">Apply to listing</button><button type="button" class="subtle" @click="resetSearch">Reset filters</button></div>
  </form>
  <details class="listing-columns tool-menu">
    <summary>Columns</summary>
    <div class="pop-menu listing-column-menu">
      <label><input v-model="browser.technical" type="checkbox" /> Storage class</label>
      <label v-for="column in detailColumns" :key="column.id"><input v-model="browser.detailColumns" type="checkbox" :value="column.id" /> {{ column.label }}</label>
      <button @click="browser.columnWidths = {}">Reset widths</button>
    </div>
  </details>
</template>

<style scoped>
.listing-options { display: grid; gap: 12px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--radius); padding: 12px; margin-bottom: 12px; }
.listing-fields { display: grid; grid-template-columns: repeat(auto-fit, minmax(114px, 1fr)); gap: 10px; }
.listing-fields label { display: block; margin-bottom: 4px; }
.listing-fields input { width: 100%; }
.listing-switches, .listing-actions { display: flex; flex-wrap: wrap; gap: 10px 14px; }
.listing-switches label, .listing-column-menu label { display: flex; align-items: center; gap: 6px; }
.listing-columns { margin-bottom: 8px; width: max-content; }
.listing-columns summary { color: var(--accent-deep); cursor: pointer; }
.listing-column-menu { left: 0; right: auto; padding: 9px; gap: 6px; }
</style>
