<script setup lang="ts">
import { computed, ref } from 'vue'
import { ArrowDown, ArrowUp, ChevronRight, File, FileCode2, FileImage, FileJson2, FileText, Folder, Table2 } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { displayName, type ObjectRow } from '../model/objects'
import { useBrowser } from '../store/browser'

const props = defineProps<{ rows: ObjectRow[]; technical: boolean; compact: boolean }>()
const emit = defineEmits<{ open: [row: ObjectRow]; next: [] }>()
const browser = useBrowser()
const sort = ref<'name' | 'size' | 'modified' | 'storageClass'>('name')
const ascending = ref(true)
const selectable = computed(() => props.rows.filter(row => !row.folder).map(row => row.key))
const allSelected = computed(() => selectable.value.length > 0 && selectable.value.every(key => browser.selected.includes(key)))

const sorted = computed(() => [...props.rows].sort(compareRows))

function compareRows(left: ObjectRow, right: ObjectRow): number {
  if (left.folder !== right.folder) return left.folder ? -1 : 1
  const direction = ascending.value ? 1 : -1
  if (sort.value === 'size') return direction * ((left.size ?? 0) - (right.size ?? 0))
  if (sort.value === 'modified') return direction * (left.modified ?? '').localeCompare(right.modified ?? '')
  if (sort.value === 'storageClass') return direction * (left.storageClass ?? '').localeCompare(right.storageClass ?? '')
  return direction * left.key.localeCompare(right.key, undefined, { numeric: true })
}

function sortBy(column: typeof sort.value): void {
  if (sort.value === column) ascending.value = !ascending.value
  else {
    sort.value = column
    ascending.value = true
  }
}

function sortState(column: typeof sort.value): 'ascending' | 'descending' | 'none' {
  return sort.value === column ? ascending.value ? 'ascending' : 'descending' : 'none'
}

function fileIcon(row: ObjectRow) {
  if (row.folder) return Folder
  const extension = row.key.split('.').at(-1)?.toLowerCase()
  if (extension === 'csv' || extension === 'parquet') return Table2
  if (extension === 'json' || extension === 'geojson' || extension === 'ndjson') return FileJson2
  if (extension === 'txt' || extension === 'md') return FileText
  if (extension === 'png' || extension === 'jpg' || extension === 'webp') return FileImage
  if (extension === 'yaml' || extension === 'xml') return FileCode2
  return File
}

function fileKind(row: ObjectRow): string {
  if (row.folder) return 'Folder'
  if (!row.key.split('/').at(-1)?.includes('.')) return 'Object'
  return row.key.split('.').at(-1)?.toUpperCase() ?? 'Object'
}
</script>

<template>
  <div class="object-table-frame" :class="{ compact }">
    <table class="object-table">
      <thead>
        <tr>
          <th class="checkbox-col"><input type="checkbox" aria-label="Select all loaded objects" :checked="allSelected" :disabled="!selectable.length" @change="browser.toggleVisible(selectable)" /></th>
          <th :aria-sort="sortState('name')"><button class="table-sort" @click="sortBy('name')">Name <component :is="ascending ? ArrowUp : ArrowDown" v-if="sort === 'name'" :size="13" /></button></th>
          <th class="type-col">Type</th>
          <th class="size-col" :aria-sort="sortState('size')"><button class="table-sort" @click="sortBy('size')">Size <component :is="ascending ? ArrowUp : ArrowDown" v-if="sort === 'size'" :size="13" /></button></th>
          <th class="date-col" :aria-sort="sortState('modified')"><button class="table-sort" @click="sortBy('modified')">Modified <component :is="ascending ? ArrowUp : ArrowDown" v-if="sort === 'modified'" :size="13" /></button></th>
          <th v-if="technical" class="technical-col" :aria-sort="sortState('storageClass')"><button class="table-sort" @click="sortBy('storageClass')">Storage class <component :is="ascending ? ArrowUp : ArrowDown" v-if="sort === 'storageClass'" :size="13" /></button></th>
          <th class="arrow-col"><span class="sr-only">Open</span></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in sorted" :key="`${row.folder}:${row.key}`" :class="{ focused: browser.focus?.key === row.key, checked: browser.selected.includes(row.key) }"
          @click="emit('open', row)">
          <td><input v-if="!row.folder" type="checkbox" :aria-label="`Select ${row.key}`" :checked="browser.selected.includes(row.key)" @click.stop @change="browser.toggleSelection(row.key)" /></td>
          <td>
            <button class="object-name" :title="row.key" @click.stop="emit('open', row)">
              <span class="file-icon" :class="{ 'folder-icon': row.folder }"><component :is="fileIcon(row)" :size="16" /></span>
              <span>{{ displayName(row.key, browser.prefix, row.folder) }}</span>
            </button>
          </td>
          <td class="muted type-col">{{ fileKind(row) }}</td>
          <td class="muted size-col">{{ formatSize(row.size) }}</td>
          <td class="muted date-col">{{ formatDate(row.modified) }}</td>
          <td v-if="technical" class="muted technical-col">{{ row.storageClass ?? '—' }}</td>
          <td class="row-chevron"><ChevronRight :size="15" aria-hidden="true" /></td>
        </tr>
      </tbody>
    </table>
    <button v-if="browser.cursor" class="load-more" :disabled="browser.loading" @click="emit('next')">{{ browser.loading ? 'Loading…' : 'Load next page' }}</button>
  </div>
</template>
