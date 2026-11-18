<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { ChevronDown, ChevronRight, Folder } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import { objectsApi } from '../api/objects'
import { displayName, type Location, type ObjectRow } from '../model/objects'

interface Branch {
  rows: ObjectRow[]
  cursor: string | null
  expanded: boolean
  loading: boolean
  loaded: boolean
  error: string
}

const props = defineProps<{ connection: string; bucket: string; prefix: string }>()
const emit = defineEmits<{ navigate: [prefix: string] }>()
const branches = reactive(new Map<string, Branch>())
const active = ref(false)
let revision = 0

watch(() => [props.connection, props.bucket], () => {
  revision++
  branches.clear()
  if (active.value) openRoot()
})

function branch(path: string): Branch {
  let item = branches.get(path)
  if (!item) {
    item = reactive({ rows: [], cursor: null, expanded: false, loading: false, loaded: false, error: '' })
    branches.set(path, item)
  }
  return item
}

async function readChildren(prefix: string, more = false): Promise<void> {
  const item = branch(prefix)
  if (item.loading) return
  const current = revision
  const target: Location = { connection: props.connection, bucket: props.bucket, prefix }
  item.loading = true
  item.error = ''
  try {
    const page = await objectsApi.list(target, more ? item.cursor : null, false)
    if (current !== revision) return
    const rows = page.rows.filter(row => row.folder && row.key !== prefix)
    item.rows = more ? [...item.rows, ...rows] : rows
    for (const row of rows) branch(row.key)
    item.cursor = page.cursor
    item.loaded = true
  } catch (failure) {
    if (current === revision) item.error = errorMessage(failure)
  } finally {
    if (current === revision) item.loading = false
  }
}

function openRoot(): void {
  branch('').expanded = true
  if (!branch('').loaded) void readChildren('')
}

function toggle(path: string): void {
  const item = branch(path)
  item.expanded = !item.expanded
  if (item.expanded && !item.loaded) void readChildren(path)
}

interface TreeRow {
  key: string
  parent: string
  depth: number
  item: Branch
}

function treeRows(path = '', parent = '', depth = 0): TreeRow[] {
  const item = branches.get(path)
  if (!item) return []
  const children = item.expanded ? item.rows.flatMap(row => treeRows(row.key, path, depth + 1)) : []
  return [{ key: path, parent, depth, item }, ...children]
}
</script>

<template>
  <details class="folder-tree" @toggle="active = ($event.target as HTMLDetailsElement).open; if (active) openRoot()">
    <summary><Folder :size="15" /> Folder tree</summary>
    <ul v-if="active" aria-label="Bucket folders">
      <li v-for="entry in treeRows()" :key="entry.key" :style="{ paddingInlineStart: entry.depth * 16 + 'px' }">
        <button v-if="entry.depth" class="icon-button subtle" :aria-label="entry.item.expanded ? 'Collapse ' + entry.key : 'Expand ' + entry.key"
          :aria-expanded="entry.item.expanded" @click="toggle(entry.key)">
          <component :is="entry.item.expanded ? ChevronDown : ChevronRight" :size="14" />
        </button>
        <button class="subtle folder-link" :title="entry.key" :aria-current="prefix === entry.key ? 'location' : undefined" @click="emit('navigate', entry.key)">
          {{ entry.depth ? displayName(entry.key, entry.parent, true) : bucket }}
        </button>
        <small v-if="entry.item.expanded && entry.item.loading" role="status">Reading…</small>
        <button v-if="entry.item.expanded && entry.item.error" class="subtle" :title="entry.item.error" @click="readChildren(entry.key, !!entry.item.cursor)">Retry folders</button>
        <button v-else-if="entry.item.expanded && entry.item.cursor" class="subtle" :disabled="entry.item.loading" @click="readChildren(entry.key, true)">More folders</button>
      </li>
    </ul>
  </details>
</template>

<style scoped>
.folder-tree { min-width: 0; }
summary { display: flex; align-items: center; gap: 6px; width: max-content; cursor: pointer; }
ul { list-style: none; margin: 7px 0 0; padding: 6px; max-height: 180px; overflow: auto; border: 1px solid var(--line); border-radius: var(--radius); background: var(--surface); }
li { display: flex; align-items: center; gap: 4px; min-width: 0; }
button { min-height: 26px; }
.folder-link { white-space: pre; overflow: hidden; text-overflow: ellipsis; display: block; }
.folder-link[aria-current] { color: var(--accent-deep); background: var(--selected); }
</style>
