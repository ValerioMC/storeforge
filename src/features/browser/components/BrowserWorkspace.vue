<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Archive, ArrowDownToLine, ArrowLeft, ChevronDown, ChevronRight, ClipboardPaste, Copy, Database, Ellipsis, Folder, FolderPlus, Layers3, ListFilter, RefreshCw, Scissors, Search, Settings2, Trash2, Upload, X } from 'lucide-vue-next'
import { formatSize } from '../../../common/format'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import BucketSettings from '../../bucket/components/BucketSettings.vue'
import { useConnections } from '../../connection/store/connections'
import { useObjectActions } from '../composables/useObjectActions'
import { displayName, objectId, parentPrefix, type ObjectId, type ObjectRow, type Tag } from '../model/objects'
import { useBrowser } from '../store/browser'
import FolderDialog from './FolderDialog.vue'
import ObjectInspector from './ObjectInspector.vue'
import ObjectTable from './ObjectTable.vue'
import TagEditor from './TagEditor.vue'
import VersionList from './VersionList.vue'

const props = defineProps<{ demo: boolean; compact: boolean }>()
const emit = defineEmits<{ add: []; demo: []; createBucket: []; tools: []; job: [id: string] }>()
const browser = useBrowser()
const connections = useConnections()
const tab = ref<'Objects' | 'Versions' | 'Settings'>('Objects')
const filter = ref('')
const technical = ref(false)
const folder = ref(false)
const bulkTags = ref(false)
const deleteItems = ref<ObjectId[]>([])
const pasteReview = ref(false)
const pathDraft = ref('')
const editPath = ref(false)
const filterField = ref<HTMLInputElement>()
let restoreSearchFocus = false
const { clipboard, failure, notice, run, enqueue, upload, download, archive, stage, paste } = useObjectActions(id => emit('job', id))

const rows = computed(() => browser.rows.filter(row => displayName(row.key, browser.prefix).toLowerCase().includes(filter.value.toLowerCase())))
const listingLocation = computed(() => JSON.stringify([connections.contextKey, browser.bucket, browser.prefix]))
const fileCount = computed(() => rows.value.filter(row => !row.folder).length)
const folderCount = computed(() => rows.value.length - fileCount.value)
const currentSize = computed(() => rows.value.reduce((sum, row) => sum + (row.size ?? 0), 0))
const pathParts = computed(() => {
  let path = ''
  return browser.prefix ? browser.prefix.slice(0, -1).split('/').map(segment => {
    path += segment + '/'
    return { name: segment || '/', path }
  }) : []
})
const heading = computed(() => tab.value === 'Settings' ? browser.bucket : pathParts.value.at(-1)?.name ?? browser.bucket)
const scope = computed(() => tab.value === 'Settings' ? 'Bucket-wide configuration' : browser.prefix ? 'Bucket prefix' : 'Bucket contents')
const pasteSourceName = computed(() => connections.profiles.find(profile => profile.id === clipboard.value?.source.connection)?.name ?? 'Removed connection')
const pasteDescription = computed(() => {
  const count = clipboard.value?.objects.length ?? 0
  return `${count} ${count === 1 ? 'object' : 'objects'} will be transferred. The job checks for existing destination keys before writing.`
})
const readonly = computed(() => props.demo || !browser.bucket)

watch(() => [browser.location.connection, browser.bucket], () => { tab.value = 'Objects' })
watch(() => connections.contextKey, () => {
  folder.value = false
  bulkTags.value = false
  deleteItems.value = []
  pasteReview.value = false
}, { flush: 'sync' })
watch(() => browser.prefix, () => { filter.value = ''; editPath.value = false })
watch(tab, value => { if (value !== 'Objects') browser.focus = null })

defineExpose({
  focusSearch,
  closeInspector,
})

function focusSearch(): void {
  filterField.value?.focus()
}

function closeInspector(): void {
  if (!browser.focus) return
  restoreSearchFocus = true
  browser.focus = null
}

function restoreInspectorFocus(): void {
  if (!restoreSearchFocus) return
  restoreSearchFocus = false
  if (!browser.focus) focusSearch()
}

function openRow(row: ObjectRow): void {
  if (row.folder) {
    void browser.navigate(browser.bucket, row.key)
  } else {
    browser.focus = objectId(browser.bucket, row.key)
  }
}

function jump(): void {
  const path = !pathDraft.value || pathDraft.value.endsWith('/') ? pathDraft.value : `${pathDraft.value}/`
  editPath.value = false
  void browser.navigate(browser.bucket, path)
}

function beginPath(): void {
  pathDraft.value = browser.prefix
  editPath.value = true
}

function beginDelete(objects: ObjectId[]): void {
  deleteItems.value = objects
}

async function remove(): Promise<void> {
  const objects = deleteItems.value
  deleteItems.value = []
  if (!objects.length) return
  const context = connections.contextKey
  const connection = browser.location.connection
  await run(async () => {
    await enqueue({ kind: 'delete', connection, objects })
    if (connections.contextKey === context) {
      browser.selected = []
      browser.focus = null
    }
  })
}

async function setTags(tags: Tag[]): Promise<void> {
  bulkTags.value = false
  await run(() => enqueue({ kind: 'tag', connection: browser.location.connection, objects: browser.selectedObjects, tags }))
}

async function removeBucket(): Promise<void> {
  await browser.navigate('')
  await browser.loadBuckets()
}

async function refresh(): Promise<void> {
  await browser.loadRows()
  await browser.loadBuckets()
}

async function acceptPaste(): Promise<void> {
  pasteReview.value = false
  await paste(browser.location)
}

function selectedStage(move: boolean): void {
  stage(browser.location, browser.selectedObjects, move)
  browser.selected = []
}

async function copyPath(): Promise<void> {
  await run(async () => {
    await navigator.clipboard.writeText(`s3://${browser.bucket}/${browser.prefix}`)
    notice.value = 'S3 path copied.'
  })
}
</script>

<template>
  <div class="browser-host">
    <FadeRegion>
      <div v-if="browser.bucket" class="browser-layout">
        <section class="listing">
          <div class="listing-content">
            <header class="listing-header">
              <nav aria-label="Current S3 path" class="breadcrumbs">
                <button class="crumb-root" :title="browser.bucket" @click="browser.navigate(browser.bucket)"><Database :size="15" />{{ browser.bucket }}</button>
                <template v-for="part in tab === 'Settings' ? [] : pathParts" :key="part.path">
                  <ChevronRight :size="13" />
                  <button :title="part.path" @click="browser.navigate(browser.bucket, part.path)">{{ part.name }}</button>
                </template>
              </nav>
              <div class="heading-line">
                <div class="title-group"><h1 :title="heading">{{ heading }}</h1><p>{{ scope }}</p></div>
                <button v-if="tab === 'Objects'" class="primary main-upload" :disabled="readonly" :title="demo ? 'The demo is read-only' : undefined" @click="upload(browser.location)"><Upload :size="17" /> Upload files</button>
              </div>
            </header>

            <nav class="workspace-tabs" aria-label="Bucket workspace">
              <button :class="{ active: tab === 'Objects' }" :aria-pressed="tab === 'Objects'" @click="tab = 'Objects'"><Folder :size="16" />Objects <span class="tab-count">{{ browser.rows.length }}</span></button>
              <button :class="{ active: tab === 'Versions' }" :aria-pressed="tab === 'Versions'" @click="tab = 'Versions'"><Layers3 :size="16" />Versions</button>
              <button :class="{ active: tab === 'Settings' }" :aria-pressed="tab === 'Settings'" @click="tab = 'Settings'"><Settings2 :size="16" />Bucket settings</button>
            </nav>

            <Notice v-if="failure" tone="error">{{ failure }}</Notice>
            <Notice v-if="notice" tone="success">{{ notice }}</Notice>
            <Notice v-if="browser.bucketError" tone="error">{{ browser.bucketError }}</Notice>

            <FadeRegion>
              <div v-if="tab === 'Objects'">
                <div class="object-tools">
                  <label class="object-search">
                    <Search :size="17" /><input ref="filterField" v-model="filter" type="search" placeholder="Filter loaded objects…" aria-label="Filter loaded objects" /><kbd>Ctrl K</kbd>
                  </label>
                  <div class="object-tool-buttons">
                    <button v-if="browser.prefix" class="icon-button" title="Parent folder" aria-label="Parent folder" @click="browser.navigate(browser.bucket, parentPrefix(browser.prefix))"><ArrowLeft :size="17" /></button>
                    <button class="button-label" title="New folder" :disabled="readonly" @click="folder = true"><FolderPlus :size="17" /><span>New folder</span></button>
                    <button class="icon-button" aria-label="Refresh objects" title="Refresh objects" :disabled="browser.loading" @click="refresh"><RefreshCw :size="16" :class="{ spinning: browser.loading }" /></button>
                    <details class="tool-menu">
                      <summary class="icon-button" title="More list actions" aria-label="More list actions"><Ellipsis :size="18" /></summary>
                      <div class="pop-menu">
                        <button @click="beginPath">Go to prefix</button>
                        <button @click="copyPath">Copy S3 path</button>
                        <button @click="browser.recursive = !browser.recursive; browser.loadRows()">{{ browser.recursive ? 'Show folders' : 'List recursively' }}</button>
                        <button @click="technical = !technical">{{ technical ? 'Hide storage class' : 'Show storage class' }}</button>
                        <button :disabled="readonly" @click="emit('tools')">Prefix metrics and tools</button>
                      </div>
                    </details>
                  </div>
                </div>
                <form v-if="editPath" class="inline path-entry" @submit.prevent="jump">
                  <label>Go to prefix<input v-model="pathDraft" placeholder="Path relative to bucket root" spellcheck="false" /></label>
                  <button class="primary">Go</button><button type="button" class="icon-button" aria-label="Cancel prefix entry" @click="editPath = false"><X :size="17" /></button>
                </form>
                <div v-if="browser.selected.length || clipboard" class="selection-bar">
                  <span v-if="browser.selected.length"><ListFilter :size="15" /> {{ browser.selected.length }} selected</span>
                  <span v-else>{{ clipboard?.objects.length }} ready to {{ clipboard?.move ? 'move' : 'copy' }}</span>
                  <div v-if="browser.selected.length" class="selection-buttons">
                    <button :disabled="readonly" @click="download(browser.location.connection, browser.selectedObjects)"><ArrowDownToLine :size="15" /><span>Download</span></button>
                    <button :disabled="readonly" @click="archive(browser.location.connection, browser.selectedObjects)"><Archive :size="15" /><span>ZIP</span></button>
                    <button :disabled="readonly" title="Stage copy" @click="selectedStage(false)"><Copy :size="15" /><span>Copy</span></button>
                    <button :disabled="readonly" title="Stage move" @click="selectedStage(true)"><Scissors :size="15" /><span>Cut</span></button>
                    <button :disabled="readonly" @click="bulkTags = true">Tags</button>
                    <button :disabled="readonly" class="danger-outline" aria-label="Delete selected objects" @click="beginDelete(browser.selectedObjects)"><Trash2 :size="15" /></button>
                    <button class="icon-button subtle" aria-label="Clear selection" @click="browser.selected = []"><X :size="15" /></button>
                  </div>
                  <div v-if="clipboard" class="selection-buttons"><button :disabled="readonly" @click="pasteReview = true"><ClipboardPaste :size="15" />Paste</button>
                    <button class="icon-button subtle" aria-label="Clear clipboard" @click="clipboard = undefined"><X :size="15" /></button>
                  </div>
                </div>

                <FadeRegion>
                  <div :key="listingLocation">
                    <Notice v-if="browser.error" tone="error">{{ browser.error }}</Notice>
                    <p v-if="browser.loading && !browser.rows.length" class="loading-line" role="status">Loading objects…</p>
                    <ObjectTable v-else-if="rows.length" :rows="rows" :technical="technical" :compact="compact" @open="openRow" @next="browser.loadRows(true)" />
                    <EmptyState v-else-if="!browser.error" :icon="Folder" :title="filter ? 'No matching objects' : 'This prefix is empty'"
                      :description="filter ? 'Clear the filter to see loaded objects.' : 'Create a folder or upload files to this location.'">
                      <button v-if="filter" @click="filter = ''">Clear filter</button>
                      <button v-else-if="!readonly" class="primary" @click="upload(browser.location)"><Upload :size="16" />Upload files</button>
                    </EmptyState>
                    <button v-if="browser.cursor && !rows.length" class="load-more" :disabled="browser.loading" @click="browser.loadRows(true)">Search next page</button>
                  </div>
                </FadeRegion>
              </div>
              <VersionList v-else-if="tab === 'Versions'" :connection="browser.location.connection" :bucket="browser.bucket" :prefix="browser.prefix" :exact="false" :demo="demo"
                @download="download(browser.location.connection, [$event])" @changed="browser.loadRows()" />
              <BucketSettings v-else :connection="browser.location.connection" :bucket="browser.bucket" :demo="demo" @removed="removeBucket" />
            </FadeRegion>
          </div>
          <footer v-if="tab === 'Objects'" class="listing-footer">
            <span>{{ fileCount }} {{ fileCount === 1 ? 'object' : 'objects' }}<span class="separator">·</span>{{ folderCount }} {{ folderCount === 1 ? 'folder' : 'folders' }}</span>
            <span>{{ formatSize(currentSize) }} in view <span v-if="browser.cursor" class="separator">· More available</span></span>
          </footer>
        </section>
        <FadeRegion @after-leave="restoreInspectorFocus">
          <ObjectInspector v-if="browser.focus && tab === 'Objects'" :connection="browser.location.connection" :object="browser.focus" :demo="demo"
            @close="closeInspector" @download="download(browser.location.connection, [$event])" @changed="browser.loadRows()" @remove="beginDelete([$event])" />
        </FadeRegion>
      </div>
      <section v-else class="workspace-page start-page">
        <div class="page-header"><div><h1>Object browser</h1><p>Browse and operate on S3 storage from your desktop.</p></div>
          <button v-if="browser.location.connection" title="Refresh buckets" :disabled="browser.bucketLoading" @click="browser.loadBuckets()"><RefreshCw :size="16" /> Refresh buckets</button>
        </div>
        <Notice v-if="browser.bucketError" tone="error">{{ browser.bucketError }}</Notice>
        <EmptyState v-if="!browser.location.connection" :icon="Database" title="Connect to your storage" description="Add an endpoint to browse its buckets and files.">
          <button class="primary" @click="emit('add')">Add connection</button><button @click="emit('demo')">Explore the demo</button>
        </EmptyState>
        <p v-else-if="browser.bucketLoading" class="loading-line" role="status">Loading buckets…</p>
        <div v-else-if="browser.buckets.length" class="settings-sheet">
          <button v-for="bucket in browser.buckets" :key="bucket.name" class="settings-row bucket-card" @click="browser.navigate(bucket.name)">
            <span class="file-icon"><Database :size="18" /></span><strong>{{ bucket.name }}</strong><ChevronRight :size="17" />
          </button>
        </div>
        <EmptyState v-else-if="!browser.bucketError" :icon="Database" title="No buckets yet" description="Create a bucket or add a known bucket to your connection profile.">
          <button class="primary" :disabled="demo" @click="emit('createBucket')">Create bucket</button>
        </EmptyState>
      </section>
    </FadeRegion>

    <FolderDialog v-if="folder" :location="browser.location" @close="folder = false" @created="folder = false; browser.loadRows()" />
    <Modal v-if="bulkTags" title="Replace tags on selected objects" @close="bulkTags = false">
      <div class="modal-form"><p class="muted">These tags will replace the tags on all {{ browser.selected.length }} selected objects.</p>
        <TagEditor :tags="[]" @save="setTags" /></div>
    </Modal>
    <ConfirmDialog v-if="deleteItems.length" title="Delete selected objects?"
      :description="deleteItems.length === 1 ? `${deleteItems[0]?.key} will be deleted from ${browser.bucket}. Versioned buckets may retain a delete marker.` : `Delete ${deleteItems.length} objects from ${browser.bucket}. Versioned buckets may retain delete markers.`"
      action="Delete objects" danger @close="deleteItems = []" @confirm="remove" />
    <ConfirmDialog v-if="pasteReview && clipboard" :title="clipboard.move ? 'Move selected objects?' : 'Copy selected objects?'"
      :description="pasteDescription"
      :action="clipboard.move ? 'Move objects' : 'Copy objects'" @close="pasteReview = false" @confirm="acceptPaste">
      <dl class="transfer-route">
        <div><dt>Source</dt><dd>{{ pasteSourceName }}<code>s3://{{ clipboard.source.bucket }}/{{ clipboard.source.prefix }}</code></dd></div>
        <div><dt>Destination</dt><dd>{{ connections.active?.name }}<code>s3://{{ browser.bucket }}/{{ browser.prefix }}</code></dd></div>
      </dl>
    </ConfirmDialog>
  </div>
</template>
