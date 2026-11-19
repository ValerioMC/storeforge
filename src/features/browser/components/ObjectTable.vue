<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch, type ComponentPublicInstance } from 'vue'
import { ArrowDown, ArrowUp, Ellipsis, File, FileCode2, FileImage, FileJson2, FileText, Folder, Table2 } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { type ResizableColumn, type SortColumn } from '../model/listing'
import { displayName, type ObjectRow } from '../model/objects'
import type { BrowserStore } from '../store/browser'

const props = defineProps<{ rows: ObjectRow[]; browser: BrowserStore; compact: boolean }>()
const selectedFolders = defineModel<string[]>('selectedFolders', { default: () => [] })
const emit = defineEmits<{ open: [row: ObjectRow]; menu: [row: ObjectRow, event: MouseEvent]; next: []; sort: [column: SortColumn] }>()
const browser = computed(() => props.browser)
const visible = ref(new Set<string>())
const elements = new Map<HTMLElement, ObjectRow>()
let observer: IntersectionObserver | undefined
const selectable = computed(() => props.rows.filter(row => !row.folder && !row.deleteMarker).map(row => row.key))
const folders = computed(() => props.rows.filter(row => row.folder).map(row => row.key))
const allSelected = computed(() => selectable.value.length + folders.value.length > 0 &&
  selectable.value.every(key => browser.value.selected.includes(key)) && folders.value.every(key => selectedFolders.value.includes(key)))
const lazy = computed(() => browser.value.detailColumns.some(column => column === 'versionId' || column === 'contentType'))
const running = ref(0)
const fixedWidths: Record<ResizableColumn, number> = {
  name: 150, type: 62, size: 66, modified: 108, storageClass: 112, contentType: 150, versionId: 130, etag: 130,
}
const labels: Record<ResizableColumn, string> = {
  name: 'Name', type: 'Type', size: 'Size', modified: 'Modified', storageClass: 'Storage class',
  contentType: 'Content type', versionId: 'Version ID', etag: 'ETag',
}
const columns = computed<ResizableColumn[]>(() => [
  'name', 'type', 'size', 'modified',
  ...(browser.value.technical ? ['storageClass'] as const : []),
  ...browser.value.detailColumns.filter(column => column !== 'storageClass'),
])
const minWidth = computed(() => 58 + columns.value.reduce((sum, column) => sum + columnWidth(column), 0))

watch([visible, lazy, running, () => browser.value.rowDetails.size], loadVisible, { deep: true })

onMounted(() => {
  if (typeof IntersectionObserver !== 'function') return
  observer = new IntersectionObserver(entries => {
    for (const entry of entries) {
      const row = entry.target instanceof HTMLElement ? elements.get(entry.target) : undefined
      if (!row) continue
      const key = row.key
      if (entry.isIntersecting) visible.value.add(key)
      else visible.value.delete(key)
    }
  })
  for (const [element, row] of elements) if (!row.folder && !row.deleteMarker) observer.observe(element)
})

onBeforeUnmount(() => observer?.disconnect())

defineExpose({ focusRow })

function toggleAll(): void {
  const remove = allSelected.value
  const objects = new Set(browser.value.selected)
  const selected = new Set(selectedFolders.value)
  for (const key of selectable.value) {
    if (remove) objects.delete(key)
    else objects.add(key)
  }
  for (const key of folders.value) {
    if (remove) selected.delete(key)
    else selected.add(key)
  }
  browser.value.selected = [...objects]
  selectedFolders.value = [...selected]
}

function toggleFolder(key: string): void {
  selectedFolders.value = selectedFolders.value.includes(key)
    ? selectedFolders.value.filter(value => value !== key)
    : [...selectedFolders.value, key]
}

function checked(row: ObjectRow): boolean {
  return row.folder ? selectedFolders.value.includes(row.key) : !row.deleteMarker && browser.value.selected.includes(row.key)
}

function toggleRow(row: ObjectRow, event: MouseEvent): void {
  if (row.folder) toggleFolder(row.key)
  else browser.value.toggleSelection(row.key, selectable.value, event.shiftKey)
}

function observeRow(row: ObjectRow, element: Element | ComponentPublicInstance | null): void {
  if (!(element instanceof HTMLElement)) {
    for (const [previous, observed] of elements) {
      if (observed.key !== row.key || observed.folder !== row.folder || observed.deleteMarker !== row.deleteMarker) continue
      observer?.unobserve(previous)
      elements.delete(previous)
    }
    if (!row.folder && !row.deleteMarker) visible.value.delete(row.key)
    return
  }
  if (elements.has(element)) return
  elements.set(element, row)
  if (row.folder || row.deleteMarker) return
  observer?.observe(element)
  if (typeof IntersectionObserver !== 'function') visible.value.add(row.key)
}

function focusRow(key: string, folder: boolean): boolean {
  const element = [...elements].find(([, row]) => row.key === key && row.folder === folder)?.[0]
  const button = element?.querySelector<HTMLButtonElement>('.object-name')
  button?.focus()
  return !!button && document.activeElement === button
}

function loadVisible(): void {
  if (!lazy.value) return
  for (const key of visible.value) {
    if (running.value >= 6) return
    if (browser.value.rowDetails.has(key)) continue
    running.value++
    void browser.value.loadRowDetails(key).finally(() => running.value--)
  }
}

function columnWidth(column: ResizableColumn): number {
  return browser.value.columnWidths[column] ?? fixedWidths[column]
}

function widthStyle(column: ResizableColumn): { width?: string } {
  if (column === 'name' && browser.value.columnWidths.name === undefined) return {}
  return { width: columnWidth(column) + 'px' }
}

function setWidth(column: ResizableColumn, width: number): void {
  browser.value.columnWidths[column] = Math.round(Math.min(720, Math.max(48, width)))
}

function resize(event: PointerEvent, column: ResizableColumn): void {
  if (event.button !== 0 || !(event.currentTarget instanceof HTMLButtonElement)) return
  const handle = event.currentTarget
  handle.setPointerCapture(event.pointerId)
  const initial = handle.closest('th')?.getBoundingClientRect().width ?? columnWidth(column)
  const start = event.clientX
  function move(next: PointerEvent): void {
    if (next.pointerId === event.pointerId) setWidth(column, initial + next.clientX - start)
  }
  function finish(): void {
    handle.removeEventListener('pointermove', move)
    handle.removeEventListener('lostpointercapture', finish)
  }
  handle.addEventListener('pointermove', move)
  handle.addEventListener('lostpointercapture', finish)
}

function resizeWithKey(event: KeyboardEvent, column: ResizableColumn): void {
  if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
  event.preventDefault()
  setWidth(column, columnWidth(column) + (event.key === 'ArrowLeft' ? -12 : 12))
}

function sortable(column: ResizableColumn): column is Exclude<SortColumn, 'provider'> {
  return column === 'name' || column === 'size' || column === 'modified' || column === 'storageClass'
}

function sortState(column: ResizableColumn): 'ascending' | 'descending' | undefined {
  return browser.value.sort.column === column ? browser.value.sort.ascending ? 'ascending' : 'descending' : undefined
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
  if (row.deleteMarker) return 'Deleted'
  if (!row.key.split('/').at(-1)?.includes('.')) return 'Object'
  return row.key.split('.').at(-1)?.toUpperCase() ?? 'Object'
}

function cell(row: ObjectRow, column: ResizableColumn): string {
  if (column === 'type') return fileKind(row)
  if (column === 'size') return formatSize(row.size)
  if (column === 'modified') return formatDate(row.modified)
  if (column === 'storageClass') return row.storageClass ?? '—'
  if (column === 'etag') return row.etag ?? '—'
  if (column === 'versionId' && row.deleteMarker) return row.deleteMarker
  if (row.folder || row.deleteMarker) return '—'
  const details = browser.value.rowDetails.get(row.key)
  if (details?.status === 'error') return 'Unavailable'
  if (details?.status === 'ready' && (column === 'contentType' || column === 'versionId')) return details.value[column] ?? '—'
  return '…'
}
</script>

<template>
  <div class="object-table-frame" :class="{ compact }">
    <table class="object-table" :style="{ minWidth: minWidth + 'px' }">
      <thead>
        <tr>
          <th class="checkbox-col"><input type="checkbox" aria-label="Select all displayed objects" :checked="allSelected" :disabled="!selectable.length && !folders.length" @change="toggleAll" /></th>
          <th v-for="column in columns" :key="column" :style="widthStyle(column)" :aria-sort="sortState(column)">
            <button v-if="sortable(column)" class="table-sort" @click="emit('sort', column)">{{ labels[column] }}
              <component :is="browser.sort.ascending ? ArrowUp : ArrowDown" v-if="browser.sort.column === column" :size="13" />
            </button>
            <span v-else>{{ labels[column] }}</span>
            <button type="button" class="column-resize" :aria-label="'Resize ' + labels[column] + ' column'" title="Drag or press arrow keys to resize"
              @pointerdown="resize($event, column)" @keydown="resizeWithKey($event, column)" />
          </th>
          <th class="arrow-col"><span class="sr-only">Open</span></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows" :key="JSON.stringify([row.folder, row.key, row.deleteMarker])"
          :ref="element => observeRow(row, element)"
          :class="{ focused: browser.focusedRow === row.key, opened: browser.focus?.key === row.key && !row.folder, checked: checked(row) }"
          @focusin="browser.focusedRow = row.key" @click="emit('open', row)" @contextmenu.prevent.stop="emit('menu', row, $event)">
          <td><input v-if="!row.deleteMarker" type="checkbox" :aria-label="`Select ${row.key}`" :checked="checked(row)"
            @click.stop="toggleRow(row, $event)" /></td>
          <td>
            <button class="object-name" :aria-current="browser.focus?.key === row.key && !row.folder ? 'true' : undefined" :title="row.key" @click.stop="emit('open', row)">
              <span class="file-icon" :class="{ 'folder-icon': row.folder }"><component :is="fileIcon(row)" :size="16" /></span>
              <span>{{ displayName(row.key, browser.prefix, row.folder) }}</span>
            </button>
          </td>
          <td v-for="column in columns.slice(1)" :key="column" class="muted" :title="cell(row, column)">{{ cell(row, column) }}</td>
          <td class="row-chevron"><button class="subtle row-actions" :aria-label="'Actions for ' + row.key" aria-haspopup="true" @click.stop="emit('menu', row, $event)"><Ellipsis :size="15" /></button></td>
        </tr>
      </tbody>
    </table>
    <button v-if="browser.cursor" class="load-more" :disabled="browser.loading" @click="emit('next')">{{ browser.loading ? 'Loading…' : 'Load next page' }}</button>
  </div>
</template>

<style scoped>
th { position: relative; }
.row-actions { min-width: 22px; min-height: 25px; padding: 0; }
.column-resize {
  position: absolute;
  inset: 0 0 0 auto;
  width: 8px;
  min-height: 0;
  padding: 0;
  border: 0;
  background: transparent;
  cursor: col-resize;
  touch-action: none;
}
.column-resize:hover, .column-resize:focus-visible { border-right: 2px solid var(--accent); }
.object-table tr.opened td:first-child { box-shadow: inset 1px 0 var(--accent); }
</style>
