<script setup lang="ts">
import { computed, nextTick, onDeactivated, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { Archive, ArrowDownToLine, ArrowLeft, ArrowRight, ChevronRight, ClipboardPaste, Copy, Database, Ellipsis, Folder, FolderPlus, Layers3, ListFilter, RefreshCw, Scissors, Search, Settings2, Star, Trash2, Upload, X } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import { formatSize } from '../../../common/format'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import BucketSettings from '../../bucket/components/BucketSettings.vue'
import { workspaceAvailable } from '../../connection/model/workspace'
import BrowserBatchMaintenance from '../../transfer/components/BrowserBatchMaintenance.vue'
import BrowserBulkAttributesModal from '../../transfer/components/BrowserBulkAttributesModal.vue'
import BrowserTransferQueue from '../../transfer/components/BrowserTransferQueue.vue'
import TransferAccessModal from '../../transfer/components/TransferAccessModal.vue'
import type { TransferAccess } from '../../transfer/model/access'
import type { BulkAttributes, VersionPlanItem, VersionRule } from '../../transfer/model/batch'
import type { Transfer } from '../../transfer/model/transfer'
import { useConnections } from '../../connection/store/connections'
import { useWorkspaceNavigation } from '../../connection/store/workspace-navigation'
import { readObjectPages } from '../api/listing'
import type { VerifiedVersion } from '../api/versions'
import { useObjectActions } from '../composables/useObjectActions'
import { neighboringRow, type InspectorTab, type MetadataDraft, type ObjectView } from '../model/inspector'
import { folderPrefix, type SortColumn } from '../model/listing'
import { objectId, parentPrefix, type Location, type ObjectId, type ObjectRow, type ObjectVersion, type Tag } from '../model/objects'
import { useBrowser, useEmbeddedBrowser } from '../store/browser'
import BrowserFavorites from './BrowserFavorites.vue'
import BrowserFolderTree from './BrowserFolderTree.vue'
import BrowserColumnMenu from './BrowserColumnMenu.vue'
import BrowserListingOptions from './BrowserListingOptions.vue'
import FolderDialog from './FolderDialog.vue'
import ObjectInspector from './ObjectInspector.vue'
import ObjectRowMenu from './ObjectRowMenu.vue'
import ObjectTable from './ObjectTable.vue'
import VersionList from './VersionList.vue'

const props = defineProps<{ demo: boolean; compact: boolean; embedded?: { connection: string; revision?: string | null; bucket: string } }>()
const emit = defineEmits<{ add: []; demo: []; createBucket: []; navigate: [bucket: string, prefix?: string]; favorite: [location: Location]; tools: []; job: [id: string] }>()
const browser = props.embedded ? useEmbeddedBrowser() : useBrowser()
const navigation = useWorkspaceNavigation()
const embeddedHistory = ref([''])
const historyIndex = ref(0)
const canBack = computed(() => props.embedded ? historyIndex.value > 0 : navigation.canBack)
const canForward = computed(() => props.embedded ? historyIndex.value < embeddedHistory.value.length - 1 : navigation.canForward)
const connections = useConnections()
const { tab, filter } = storeToRefs(browser)
const folder = ref(false)
const attributes = ref(false)
const secureTransfer = ref<{ kind: 'upload' | 'download'; target: Location; objects: ObjectId[] }>()
const secureTarget = computed(() => {
  const transfer = secureTransfer.value
  if (!transfer) return ''
  if (transfer.kind === 'download' && transfer.objects.length === 1) return transfer.objects[0]?.key ?? ''
  return 's3://' + transfer.target.bucket + '/' + transfer.target.prefix
})
const maintenance = ref(false)
const selectedFolders = ref<string[]>([])
const operationIds = ref<string[]>([])
const submitting = ref(false)
const renameObject = ref<ObjectId>()
const renameDraft = ref('')
const deleteItems = ref<ObjectId[]>([])
const deleteFolders = ref<string[]>([])
const pasteReview = ref(false)
const pathDraft = ref('')
const editPath = ref(false)
const filterField = ref<HTMLInputElement>()
const pathField = ref<HTMLInputElement>()
const pathSuggestions = ref<string[]>([])
const suggestionsError = ref('')
const pathDetails = ref(false)
const searchError = ref('')
const inspector = ref<InstanceType<typeof ObjectInspector>>()
const objectTable = ref<InstanceType<typeof ObjectTable>>()
const versionsTab = ref<HTMLButtonElement>()
const rowMenu = ref<{ row: ObjectRow; x: number; y: number }>()
let returnFocus: (() => void) | undefined
let versionOpener: HTMLElement | undefined
const { clipboard, failure, notice, run, enqueue, upload, download, archive, stage, paste } = useObjectActions(rememberOperation)

const rows = computed(() => browser.rows)
const listingLocation = computed(() => JSON.stringify([browser.location.connection, browser.revision, browser.bucket, browser.prefix]))
const previousObject = computed(() => neighboringObject(-1))
const nextObject = computed(() => neighboringObject(1))
const recents = computed(() => browser.recent.filter(location => location.connection === browser.location.connection &&
  location.bucket === browser.bucket && location.prefix !== browser.prefix).map(location => location.prefix))
const suggestedPaths = computed(() => [...new Set([...pathSuggestions.value, ...recents.value.filter(path => path.startsWith(pathDraft.value))])].slice(0, 8))
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
const selectedCount = computed(() => browser.selected.length + selectedFolders.value.length)
const stagedCount = computed(() => (clipboard.value?.objects.length ?? 0) + (clipboard.value?.folders.length ?? 0))
const renameReady = computed(() => !!renameDraft.value && !renameDraft.value.includes('/') &&
  renameDraft.value !== '.' && renameDraft.value !== '..' &&
  renameDraft.value !== renameObject.value?.key.split('/').at(-1))
const pasteDescription = computed(() => {
  const files = clipboard.value?.objects.length ?? 0
  const folders = clipboard.value?.folders.length ?? 0
  return files + ' files and ' + folders + ' folders are staged. Existing destination keys are skipped.'
})
const operationLocation = computed(() => browser.location)
const pasteTargetName = computed(() => connections.active?.name)
const pasteTargetPath = computed(() => 's3://' + browser.bucket + '/' + browser.prefix)
const deleteDescription = computed(() => deleteFolders.value.length
  ? 'Delete ' + deleteFolders.value.length + ' folders, including everything inside them, and ' + deleteItems.value.length + ' objects from ' + browser.bucket + '. Versioned buckets may retain delete markers.'
  : deleteItems.value.length === 1
    ? deleteItems.value[0]?.key + ' will be deleted from ' + browser.bucket + '. Versioned buckets may retain a delete marker.'
    : 'Delete ' + deleteItems.value.length + ' objects from ' + browser.bucket + '. Versioned buckets may retain delete markers.')
const managerAvailable = computed(() => workspaceAvailable(connections.active, 'manager'))
const readonly = computed(() => props.demo || !browser.bucket)

watch(() => connections.contextKey, () => {
  folder.value = false
  attributes.value = false
  cancelDelete()
  pasteReview.value = false
}, { flush: 'sync' })
watch(listingLocation, () => {
  secureTransfer.value = undefined
  selectedFolders.value = []
  attributes.value = false
  maintenance.value = false
  renameObject.value = undefined
  pasteReview.value = false
  cancelDelete()
  rowMenu.value = undefined
  returnFocus = undefined
  versionOpener = undefined
  editPath.value = false
  pathDetails.value = false
  searchError.value = ''
})
watch(() => props.embedded, async (target, _, cleanup) => {
  if (!target) return
  let current = true
  cleanup(() => { current = false })
  historyIndex.value = 0
  embeddedHistory.value = ['']
  await browser.changeConnection(target.connection, target.revision ?? undefined)
  if (current) await browser.navigate(target.bucket)
}, { immediate: true })
watch([pathDraft, editPath], ([draft, open], _, cleanup) => {
  if (!open) return
  let current = true
  const timer = window.setTimeout(() => void suggestPrefixes(draft, () => current), 180)
  cleanup(() => { current = false; window.clearTimeout(timer) })
})
onDeactivated(() => {
  editPath.value = false
  rowMenu.value = undefined
})

defineExpose({
  focusSearch,
  closeInspector,
})

function focusSearch(): void {
  filterField.value?.focus()
}

function closeInspector(): void {
  const view = browser.focus
  if (!view) return
  leaveInspector(() => {
    returnFocus = tab.value === 'Objects'
      ? () => focusObjectRow(view.key)
      : focusVersionOpener
    browser.focus = null
  })
}

function focusVersionOpener(): void {
  if (versionOpener?.isConnected) versionOpener.focus()
  else versionsTab.value?.focus()
}

function restoreInspectorFocus(): void {
  if (!browser.focus) returnFocus?.()
  returnFocus = undefined
}

function leaveInspector(action: () => void): void {
  if (browser.focus && inspector.value) inspector.value.requestExit(action)
  else action()
}

function focusObjectRow(key: string, folder = false): void {
  if (!objectTable.value?.focusRow(key, folder)) focusSearch()
}

function neighboringObject(direction: -1 | 1): ObjectRow | undefined {
  if (tab.value === 'Objects' && browser.focus) return neighboringRow(rows.value, browser.focus, direction)
}

function moveObject(direction: -1 | 1): void {
  const row = neighboringObject(direction)
  if (row) openRow(row)
}

function openRow(row: ObjectRow, initialTab?: InspectorTab): void {
  if (row.folder) navigate(browser.bucket, row.key)
  else openObject({ ...objectId(browser.bucket, row.key, row.deleteMarker ?? null), deleted: row.deleteMarker !== undefined, initialTab })
}

function openVersion(version: ObjectVersion): void {
  versionOpener = document.activeElement instanceof HTMLElement ? document.activeElement : undefined
  openObject(objectId(browser.bucket, version.key, version.versionId))
}

function openObject(view: ObjectView): void {
  const current = browser.focus
  if (current?.key === view.key && current.versionId === view.versionId && !!current.deleted === !!view.deleted) {
    if (view.initialTab) inspector.value?.openTab(view.initialTab)
    inspector.value?.focusHeader()
    return
  }
  leaveInspector(() => {
    browser.focus = view
    void nextTick(() => inspector.value?.focusHeader())
  })
}

function changeTab(next: typeof tab.value): void {
  if (tab.value === next) return
  leaveInspector(() => {
    browser.focus = null
    tab.value = next
  })
}

function openRowMenu(row: ObjectRow, event: MouseEvent): void {
  const anchor = event.target instanceof Element ? event.target.closest('.row-actions')?.getBoundingClientRect() : undefined
  rowMenu.value = { row, x: anchor?.left ?? event.clientX, y: anchor?.bottom ?? event.clientY }
}

function closeRowMenu(restoreFocus = false): void {
  const row = rowMenu.value?.row
  rowMenu.value = undefined
  if (restoreFocus && row) focusObjectRow(row.key, row.folder)
}

function actOnRow(action: (row: ObjectRow) => void): void {
  const row = rowMenu.value?.row
  closeRowMenu(true)
  if (row) action(row)
}

function rememberOperation(id: string): void {
  operationIds.value = [id, ...operationIds.value].slice(0, 40)
  emit('job', id)
}

function openRename(row: ObjectRow): void {
  if (row.folder || row.deleteMarker) return
  failure.value = ''
  renameObject.value = objectId(browser.bucket, row.key)
  renameDraft.value = row.key.split('/').at(-1) ?? ''
}

function renameMenuRow(): void {
  actOnRow(openRename)
}

function stageMenuRow(move: boolean): void {
  actOnRow(row => stage(selectionSource([objectId(browser.bucket, row.key)]), browser.revision,
    row.folder ? [] : [objectId(browser.bucket, row.key)], row.folder ? [row.key] : [], move))
}

function openMenuRow(initialTab?: InspectorTab): void {
  actOnRow(row => openRow(row, initialTab))
}

function downloadMenuRow(): void {
  actOnRow(row => void download(browser.location.connection, [objectId(browser.bucket, row.key)]))
}

function copyMenuRow(): void {
  actOnRow(row => void run(async () => {
    await navigator.clipboard.writeText(row.key)
    notice.value = row.folder ? 'Prefix copied.' : 'Object key copied.'
  }))
}

function saveMetadataDraft(key: string, draft?: MetadataDraft): void {
  if (draft) browser.metadataDrafts.set(key, draft)
  else browser.metadataDrafts.delete(key)
}

function saveTagDraft(key: string, draft?: Tag[]): void {
  if (draft) browser.tagDrafts.set(key, draft)
  else browser.tagDrafts.delete(key)
}

function objectChanged(): void {
  browser.invalidateRowDetails()
  void browser.loadRows()
}

function versionChanged(state: VerifiedVersion): void {
  const view = browser.focus
  if (view?.deleted && view.key === state.key && view.versionId === state.removedVersionId) {
    const latest = state.latest
    if (latest) {
      const next = objectId(view.bucket, view.key, latest.deleteMarker ? latest.versionId : null)
      browser.focus = { ...next, deleted: latest.deleteMarker, initialTab: 'Versions' }
    } else {
      closeInspector()
    }
  } else if (view && view.key === state.key && view.versionId === state.removedVersionId) {
    closeInspector()
  }
  objectChanged()
}

function navigate(bucket: string, prefix = ''): void {
  if (bucket === browser.bucket && prefix === browser.prefix) return
  leaveInspector(() => navigateToPath(bucket, prefix))
}

function openFavorite(location: Location): void {
  leaveInspector(() => emit('favorite', location))
}

function navigateToPath(bucket: string, prefix: string): void {
  if (!props.embedded) {
    emit('navigate', bucket, prefix)
    return
  }
  if (prefix === browser.prefix) return
  embeddedHistory.value = [...embeddedHistory.value.slice(0, historyIndex.value + 1), prefix]
  historyIndex.value++
  void browser.navigate(bucket, prefix)
}

function moveHistory(direction: -1 | 1): void {
  leaveInspector(() => followHistory(direction))
}

function followHistory(direction: -1 | 1): void {
  if (!props.embedded) {
    if (direction < 0) navigation.back()
    else navigation.forward()
    return
  }
  const index = historyIndex.value + direction
  const path = embeddedHistory.value[index]
  if (path === undefined) return
  historyIndex.value = index
  void browser.navigate(browser.bucket, path)
}

function jump(path = pathDraft.value): void {
  editPath.value = false
  navigate(browser.bucket, folderPrefix(path))
}

function beginPath(): void {
  pathDraft.value = browser.prefix
  pathSuggestions.value = []
  editPath.value = true
  void nextTick(() => pathField.value?.focus())
}

async function suggestPrefixes(draft: string, current: () => boolean): Promise<void> {
  suggestionsError.value = ''
  const parent = draft.slice(0, draft.lastIndexOf('/') + 1)
  const matches: string[] = []
  try {
    for await (const rows of readObjectPages({ ...browser.location, prefix: parent }, false, current)) {
      matches.push(...rows.filter(row => row.folder && row.key.startsWith(draft)).map(row => row.key))
      if (matches.length >= 8) break
    }
    if (current()) pathSuggestions.value = matches.slice(0, 8)
  } catch (failure) {
    if (current()) suggestionsError.value = errorMessage(failure)
  }
}

function applyListing(): void {
  if (!Number.isInteger(browser.options.limit) || browser.options.limit < 1 || browser.options.limit > 5000) {
    searchError.value = 'Enter a result limit between 1 and 5,000.'
    return
  }
  searchError.value = ''
  void browser.loadRows()
}

function clearFilter(): void {
  filter.value = ''
  applyListing()
}

function sortBy(column: SortColumn): void {
  browser.sort = { column, ascending: browser.sort.column !== column || !browser.sort.ascending }
  applyListing()
}

function beginDelete(objects: ObjectId[], folders: string[]): void {
  const stage = () => { deleteItems.value = objects; deleteFolders.value = folders }
  if (browser.focus && includesObject(objects, browser.focus)) leaveInspector(stage)
  else stage()
}

function cancelDelete(): void {
  deleteItems.value = []
  deleteFolders.value = []
}

function includesObject(objects: ObjectId[], current: ObjectId): boolean {
  return objects.some(object => object.bucket === current.bucket && object.key === current.key && object.versionId === current.versionId)
}

async function remove(): Promise<void> {
  const objects = deleteItems.value
  const folders = deleteFolders.value
  cancelDelete()
  if (!objects.length && !folders.length) return
  const context = listingLocation.value
  const connection = browser.location.connection
  const bucket = browser.bucket
  await run(async () => {
    await enqueue({ kind: 'delete', connection, objects, folders, bucket })
    if (listingLocation.value === context) {
      selectedFolders.value = selectedFolders.value.filter(key => !folders.includes(key))
      browser.selected = browser.selected.filter(key => !includesObject(objects, objectId(browser.bucket, key)))
      if (browser.focus && includesObject(objects, browser.focus)) closeInspector()
    }
  })
}

async function submitOperation(transfer: Transfer, close: () => void): Promise<void> {
  if (submitting.value) return
  submitting.value = true
  try {
    await run(async () => {
      await enqueue(transfer)
      close()
    })
  } finally {
    submitting.value = false
  }
}

async function setAttributes(update: BulkAttributes): Promise<void> {
  const objects = browser.selectedObjects
  await submitOperation({ kind: 'attributes', source: selectionSource(objects), objects, folders: selectedFolders.value, update },
    () => { attributes.value = false })
}

async function applyVersionPlan(rule: VersionRule, items: VersionPlanItem[]): Promise<void> {
  await submitOperation({ kind: 'applyVersionPlan', source: browser.location, rule, items },
    () => { maintenance.value = false })
}

async function rename(): Promise<void> {
  const object = renameObject.value
  if (!renameReady.value || !object) return
  const source = selectionSource([object])
  const target = { ...browser.location, prefix: object.key.slice(0, object.key.lastIndexOf('/') + 1) }
  await submitOperation({ kind: 'copy', source, target, objects: [object], renameTo: renameDraft.value, moveSource: true, overwrite: false },
    () => { renameObject.value = undefined })
}

async function removeBucket(): Promise<void> {
  const removed = browser.bucket
  navigate('')
  browser.forgetBucket(removed)
  await browser.loadBuckets()
}

async function refresh(): Promise<void> {
  browser.invalidateRowDetails()
  await browser.loadRows()
  if (!props.embedded) await browser.loadBuckets()
}

async function acceptPaste(): Promise<void> {
  pasteReview.value = false
  const profile = connections.profiles.find(item => item.id === clipboard.value?.source.connection)
  await paste(browser.location, profile?.revision ?? (profile ? null : undefined))
}

function selectionSource(objects: ObjectId[]): Location {
  return objects.every(object => object.key.startsWith(browser.prefix))
    ? browser.location
    : { ...browser.location, prefix: '' }
}

function clearSelection(): void {
  browser.selected = []
  selectedFolders.value = []
}

function selectedStage(move: boolean): void {
  stage(selectionSource(browser.selectedObjects), browser.revision, browser.selectedObjects, selectedFolders.value, move)
  clearSelection()
}

function chooseSecureTransfer(kind: 'upload' | 'download', objects: ObjectId[] = []): void {
  secureTransfer.value = { kind, target: { ...browser.location }, objects }
}

function chooseSecureObjectDownload(object: ObjectId): void {
  chooseSecureTransfer('download', [object])
}

async function startSecureTransfer(access: TransferAccess): Promise<void> {
  const transfer = secureTransfer.value
  secureTransfer.value = undefined
  if (!transfer) return
  if (transfer.kind === 'upload') await upload(transfer.target, access)
  else await download(transfer.target.connection, transfer.objects, access)
}

function archiveSelection(): void {
  void archive(selectionSource(browser.selectedObjects), browser.selectedObjects, selectedFolders.value)
}

async function copyPath(): Promise<void> {
  await run(async () => {
    await navigator.clipboard.writeText(`s3://${browser.bucket}/${browser.prefix}`)
    notice.value = 'S3 path copied.'
  })
}
</script>

<template>
  <div class="browser-host" :class="{ embedded }">
    <FadeRegion>
      <div v-if="browser.bucket" class="browser-layout">
        <section class="listing">
          <div class="listing-content">
            <header class="listing-header">
              <nav aria-label="Current S3 path" class="breadcrumbs">
                <button class="icon-button" aria-label="Browser back" :disabled="!canBack" @click="moveHistory(-1)"><ArrowLeft :size="14" /></button>
                <button class="icon-button" aria-label="Browser forward" :disabled="!canForward" @click="moveHistory(1)"><ArrowRight :size="14" /></button>
                <button class="crumb-root" :title="browser.bucket" @click="navigate(browser.bucket)"><Database :size="15" />{{ browser.bucket }}</button>
                <template v-for="part in tab === 'Settings' ? [] : pathParts" :key="part.path">
                  <ChevronRight :size="13" />
                  <button :title="part.path" @click="navigate(browser.bucket, part.path)">{{ part.name }}</button>
                </template>
              </nav>
              <div class="heading-line">
                <div class="title-group"><h1 :title="heading">{{ heading }}</h1><p>{{ scope }}</p></div>
                <button v-if="tab === 'Objects'" class="primary main-upload" :disabled="readonly" :title="demo ? 'The demo is read-only' : undefined" @click="upload(browser.location)"><Upload :size="17" /> Upload files</button>
              </div>
            </header>

            <nav v-if="!embedded" class="workspace-tabs" aria-label="Bucket workspace">
              <button :class="{ active: tab === 'Objects' }" :aria-pressed="tab === 'Objects'" @click="changeTab('Objects')"><Folder :size="16" />Objects <span class="tab-count">{{ browser.rows.length }}</span></button>
              <button ref="versionsTab" :class="{ active: tab === 'Versions' }" :aria-pressed="tab === 'Versions'" @click="changeTab('Versions')"><Layers3 :size="16" />Versions</button>
              <button :class="{ active: tab === 'Settings' }" :aria-pressed="tab === 'Settings'" @click="changeTab('Settings')"><Settings2 :size="16" />Bucket settings</button>
            </nav>

            <Notice v-if="failure" tone="error">{{ failure }}</Notice>
            <Notice v-if="notice" tone="success">{{ notice }}</Notice>
            <Notice v-if="browser.bucketError" tone="error">{{ browser.bucketError }}</Notice>

            <FadeRegion>
              <div v-if="tab === 'Objects'" class="objects-pane">
                <div class="object-tools">
                  <form class="object-search" role="search" @submit.prevent="applyListing">
                    <Search :size="17" /><input ref="filterField" v-model="filter" type="search" placeholder="Search objects…" aria-label="Search objects" />
                    <button class="subtle" type="submit">Search</button>
                  </form>
                  <div class="object-tool-buttons">
                    <button v-if="browser.prefix" class="icon-button" title="Parent folder" aria-label="Parent folder" @click="navigate(browser.bucket, parentPrefix(browser.prefix))"><ArrowLeft :size="17" /></button>
                    <button class="button-label" @click="beginPath">Edit path</button>
                    <button class="icon-button" title="Advanced listing" aria-label="Advanced listing" :aria-expanded="browser.advanced" @click="browser.advanced = !browser.advanced"><ListFilter :size="16" /></button>
                    <button v-if="!embedded" class="icon-button" :aria-label="browser.isFavorite ? 'Remove current favorite' : 'Save current path as a favorite'" :aria-pressed="browser.isFavorite" @click="browser.toggleFavorite()"><Star :size="16" :fill="browser.isFavorite ? 'currentColor' : 'none'" /></button>
                    <button class="button-label" title="New folder" :disabled="readonly" @click="folder = true"><FolderPlus :size="17" /><span>New folder</span></button>
                    <button class="icon-button" aria-label="Refresh objects" title="Refresh objects" :disabled="browser.loading" @click="refresh"><RefreshCw :size="16" :class="{ spinning: browser.loading }" /></button>
                    <details class="tool-menu">
                      <summary class="icon-button" title="More list actions" aria-label="More list actions"><Ellipsis :size="18" /></summary>
                      <div class="pop-menu">
                        <button @click="copyPath">Copy S3 path</button>
                        <button :disabled="readonly" @click="chooseSecureTransfer('upload')">Upload with options</button>
                        <button :disabled="readonly || !browser.selected.length || !!selectedFolders.length"
                          @click="chooseSecureTransfer('download', browser.selectedObjects)">Download selected with options</button>
                        <button :disabled="readonly" @click="failure = ''; maintenance = true">Prefix operations</button>
                        <button @click="pathDetails = !pathDetails">{{ pathDetails ? 'Hide path details' : 'Show path details' }}</button>
                        <button v-if="!embedded" :disabled="readonly || !managerAvailable" @click="emit('tools')">Prefix metrics and tools</button>
                      </div>
                    </details>
                  </div>
                </div>
                <div v-if="editPath" class="browser-path-editor">
                  <form class="inline path-entry" @submit.prevent="jump()" @keydown.esc.stop="editPath = false">
                    <label>Path from bucket root<input ref="pathField" v-model="pathDraft" spellcheck="false" autocomplete="off" /></label>
                    <button class="primary">Go</button><button type="button" class="icon-button" aria-label="Cancel prefix entry" @click="editPath = false"><X :size="17" /></button>
                  </form>
                  <Notice v-if="suggestionsError" tone="error">{{ suggestionsError }}</Notice>
                  <div v-if="suggestedPaths.length" class="path-choices" aria-label="Suggested paths">
                    <button v-for="path in suggestedPaths" :key="path" class="subtle" @click="jump(path)"><code>{{ path || '/' }}</code></button>
                  </div>
                  <details v-if="recents.length"><summary>Recent paths</summary>
                    <div class="path-choices"><button v-for="path in recents" :key="path" class="subtle" @click="jump(path)"><code>{{ path || '/' }}</code></button></div>
                  </details>
                </div>
                <div class="browser-path-tools">
                  <BrowserFolderTree :key="connections.contextKey" :connection="browser.location.connection" :bucket="browser.bucket" :prefix="browser.prefix" @navigate="navigate(browser.bucket, $event)" />
                  <BrowserFavorites v-if="!embedded" :favorites="browser.favorites" @open="openFavorite" @remove="browser.toggleFavorite($event)" />
                  <BrowserColumnMenu :browser="browser" />
                </div>
                <div v-if="pathDetails" class="browser-path-details">
                  <dl>
                    <div><dt>Bucket</dt><dd>{{ browser.bucket }}</dd></div>
                    <div><dt>Literal prefix</dt><dd><code>{{ JSON.stringify(browser.prefix) }}</code></dd></div>
                    <div><dt>Connection</dt><dd>{{ connections.active?.name }}</dd></div>
                  </dl>
                  <button :disabled="browser.prefixCount.status === 'loading'" @click="browser.loadPrefixCount()">Count recursively</button>
                  <span v-if="browser.prefixCount.status === 'loading'" role="status">Reading all objects in the prefix…</span>
                  <span v-else-if="browser.prefixCount.status === 'ready'" role="status">{{ browser.prefixCount.objects }} objects · {{ formatSize(browser.prefixCount.bytes) }}</span>
                  <Notice v-else-if="browser.prefixCount.status === 'error'" tone="error">{{ browser.prefixCount.message }}</Notice>
                </div>
                <BrowserListingOptions :browser="browser" @apply="applyListing" @reset="clearFilter" />
                <Notice v-if="searchError" tone="error">{{ searchError }}</Notice>
                <div v-if="selectedCount || clipboard" class="selection-bar">
                  <span v-if="selectedCount"><ListFilter :size="15" /> {{ selectedCount }} selected</span>
                  <span v-else>{{ stagedCount }} ready to {{ clipboard?.move ? 'move' : 'copy' }}</span>
                  <div v-if="selectedCount" class="selection-buttons">
                    <button :disabled="readonly || !browser.selected.length || !!selectedFolders.length" @click="download(browser.location.connection, browser.selectedObjects)"><ArrowDownToLine :size="15" /><span>Download</span></button>
                    <button :disabled="readonly" @click="archiveSelection"><Archive :size="15" /><span>ZIP</span></button>
                    <button :disabled="readonly" title="Stage copy" @click="selectedStage(false)"><Copy :size="15" /><span>Copy</span></button>
                    <button :disabled="readonly" title="Stage move" @click="selectedStage(true)"><Scissors :size="15" /><span>Cut</span></button>
                    <button :disabled="readonly" @click="failure = ''; attributes = true">Attributes</button>
                    <button :disabled="readonly" class="danger-outline" aria-label="Delete selected items" @click="beginDelete(browser.selectedObjects, selectedFolders)"><Trash2 :size="15" /></button>
                    <button class="icon-button subtle" aria-label="Clear selection" @click="clearSelection"><X :size="15" /></button>
                  </div>
                  <div v-if="clipboard" class="selection-buttons"><button :disabled="readonly" @click="pasteReview = true"><ClipboardPaste :size="15" />Paste</button>
                    <button class="icon-button subtle" aria-label="Clear clipboard" @click="clipboard = undefined"><X :size="15" /></button>
                  </div>
                </div>

                <BrowserTransferQueue v-if="!demo" :operation-ids="operationIds" />
                <FadeRegion>
                  <div :key="listingLocation" class="objects-listing">
                    <Notice v-if="browser.error" tone="error">{{ browser.error }}</Notice>
                    <Notice v-if="browser.deletedError" tone="error">Deleted objects unavailable. {{ browser.deletedError }}</Notice>
                    <p v-if="browser.loading" class="loading-line" role="status">Reading objects…</p>
                    <ObjectTable v-if="rows.length" ref="objectTable" v-model:selected-folders="selectedFolders" :browser="browser" :rows="rows" :compact="compact" @open="openRow" @menu="openRowMenu" @next="browser.loadRows(true)" @sort="sortBy" />
                    <EmptyState v-else-if="!browser.error && !browser.loading" :icon="Folder" :title="filter ? 'No matching objects' : 'This prefix is empty'"
                      :description="filter ? 'Try another search or clear the search text.' : 'Create a folder or upload files to this location.'">
                      <button v-if="filter" @click="clearFilter">Clear filter</button>
                      <button v-else-if="!readonly" class="primary" @click="upload(browser.location)"><Upload :size="16" />Upload files</button>
                    </EmptyState>
                    <button v-if="browser.cursor && !rows.length" class="load-more" :disabled="browser.loading" @click="browser.loadRows(true)">Search next page</button>
                  </div>
                </FadeRegion>
              </div>
              <VersionList v-else-if="tab === 'Versions'" :connection="browser.location.connection" :revision="browser.revision" :bucket="browser.bucket" :prefix="browser.prefix" :exact="false" :demo="demo"
                @download="download(browser.location.connection, [$event])" @view="openVersion" @changed="versionChanged" />
              <BucketSettings v-else :connection="browser.location.connection" :bucket="browser.bucket" :demo="demo" @removed="removeBucket" />
            </FadeRegion>
          </div>
          <footer v-if="tab === 'Objects'" class="listing-footer">
            <span>{{ fileCount }} {{ fileCount === 1 ? 'object' : 'objects' }}<span class="separator">·</span>{{ folderCount }} {{ folderCount === 1 ? 'folder' : 'folders' }}</span>
            <span>{{ formatSize(currentSize) }} in view <span v-if="browser.cursor" class="separator">· More available</span><span v-else-if="browser.limitReached" class="separator">· Result limit reached</span></span>
          </footer>
        </section>
        <FadeRegion @after-leave="restoreInspectorFocus">
          <ObjectInspector v-if="browser.focus && tab !== 'Settings'" ref="inspector" :connection="browser.location.connection" :revision="browser.revision"
            :object="browser.focus" :deleted="browser.focus.deleted" :initial-tab="browser.focus.initialTab" :demo="demo"
            :metadata-drafts="browser.metadataDrafts" :tag-drafts="browser.tagDrafts" :has-previous="!!previousObject" :has-next="!!nextObject"
            @close="closeInspector" @move="moveObject" @download="download(browser.location.connection, [$event])" @secure-download="chooseSecureObjectDownload" @changed="objectChanged"
            @version-changed="versionChanged" @remove="beginDelete([$event], [])" @metadata-draft="saveMetadataDraft" @tag-draft="saveTagDraft" />
        </FadeRegion>
      </div>
      <section v-else class="workspace-page start-page">
        <div class="page-header"><div><h1>Object browser</h1><p>Browse and operate on S3 storage from your desktop.</p></div>
          <button v-if="browser.location.connection" title="Refresh buckets" :disabled="browser.bucketLoading" @click="browser.loadBuckets()"><RefreshCw :size="16" /> Refresh buckets</button>
        </div>
        <BrowserFavorites v-if="!embedded" :favorites="browser.favorites" @open="emit('favorite', $event)" @remove="browser.toggleFavorite($event)" />
        <Notice v-if="browser.bucketError" tone="error">{{ browser.bucketError }}</Notice>
        <EmptyState v-if="!browser.location.connection" :icon="Database" title="Connect to your storage" description="Add an endpoint to browse its buckets and files.">
          <button class="primary" @click="emit('add')">Add connection</button><button @click="emit('demo')">Explore the demo</button>
        </EmptyState>
        <p v-else-if="browser.bucketLoading" class="loading-line" role="status">Loading buckets…</p>
        <div v-else-if="browser.buckets.length" class="settings-sheet">
          <button v-for="bucket in browser.buckets" :key="bucket.name" class="settings-row bucket-card" @click="navigate(bucket.name)">
            <span class="file-icon"><Database :size="18" /></span><strong>{{ bucket.name }}</strong><ChevronRight :size="17" />
          </button>
        </div>
        <EmptyState v-else-if="!browser.bucketError" :icon="Database" title="No buckets yet" description="Create a bucket or add a known bucket to your connection profile.">
          <button class="primary" :disabled="demo" @click="emit('createBucket')">Create bucket</button>
        </EmptyState>
      </section>
    </FadeRegion>

    <ObjectRowMenu v-if="rowMenu" v-bind="rowMenu" :demo="demo" @close="closeRowMenu" @open="openMenuRow" @download="downloadMenuRow" @copy="copyMenuRow" @stage="stageMenuRow" @rename="renameMenuRow" />

    <FolderDialog v-if="folder" :location="browser.location" @close="folder = false" @created="folder = false; browser.loadRows()" />
    <TransferAccessModal v-if="secureTransfer" :operation="secureTransfer.kind" :target="secureTarget"
      @close="secureTransfer = undefined" :perform="startSecureTransfer" />
    <BrowserBulkAttributesModal v-if="attributes" :objects="browser.selected.length" :folders="selectedFolders.length"
      :busy="submitting" :path="'s3://' + browser.bucket + '/' + selectionSource(browser.selectedObjects).prefix"
      :error="failure" @close="attributes = false" @apply="setAttributes" />
    <BrowserBatchMaintenance v-if="maintenance" :location="operationLocation" :submitting="submitting" :job-error="failure"
      @close="maintenance = false" @job="applyVersionPlan" />
    <Modal v-if="renameObject" title="Rename object" @close="renameObject = undefined">
      <form class="modal-form" @submit.prevent="rename">
        <p><code class="rename-key">{{ renameObject.key }}</code></p>
        <label>New name<input v-model="renameDraft" required :disabled="submitting" /></label>
        <p class="muted">The source is deleted after the provider confirms the new key.</p>
        <Notice v-if="failure" tone="error">{{ failure }}</Notice>
        <footer class="modal-actions"><button type="button" @click="renameObject = undefined">Cancel</button>
          <button class="primary" :disabled="submitting || !renameReady">Rename object</button>
        </footer>
      </form>
    </Modal>
    <ConfirmDialog v-if="deleteItems.length || deleteFolders.length" title="Delete selected items?"
      :description="deleteDescription"
      action="Delete items" danger @close="cancelDelete" @confirm="remove" />
    <ConfirmDialog v-if="pasteReview && clipboard" :title="clipboard.move ? 'Move selected objects?' : 'Copy selected objects?'"
      :description="pasteDescription"
      :action="clipboard.move ? 'Move objects' : 'Copy objects'" @close="pasteReview = false" @confirm="acceptPaste">
      <dl class="transfer-route">
        <div><dt>Source</dt><dd>{{ pasteSourceName }}<code>s3://{{ clipboard.source.bucket }}/{{ clipboard.source.prefix }}</code></dd></div>
        <div><dt>Destination</dt><dd>{{ pasteTargetName }}<code>{{ pasteTargetPath }}</code></dd></div>
      </dl>
    </ConfirmDialog>
  </div>
</template>

<style scoped>
.browser-host.embedded { height: min(650px, 68vh); }
.browser-host.embedded .listing-content { padding: 10px; }
.browser-host.embedded .listing-header { margin-bottom: 12px; }
.browser-host.embedded .browser-layout { grid-template-columns: minmax(0, 1fr); }
.browser-host.embedded .browser-layout:has(> .inspector) .listing { display: none; }
.object-tools, .selection-buttons { flex-wrap: wrap; }
.rename-key { white-space: pre-wrap; overflow-wrap: anywhere; }
.object-search { max-width: clamp(340px, 42%, 680px); }
.object-search button { min-height: 27px; }
.browser-path-tools { display: flex; flex-wrap: wrap; align-items: flex-start; gap: 7px; margin-bottom: 12px; }
.browser-path-editor, .browser-path-details { background: var(--surface); border: 1px solid var(--line); border-radius: var(--radius); padding: 9px; margin-bottom: 10px; }
.path-choices { display: flex; flex-wrap: wrap; gap: 4px; }
.path-choices code, .browser-path-details code { white-space: pre-wrap; overflow-wrap: anywhere; }
.browser-path-details dl { display: flex; flex-wrap: wrap; gap: 10px 18px; margin-bottom: 8px; }
.browser-path-details span { margin-inline-start: 8px; }
</style>
