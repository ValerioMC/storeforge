<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { ChevronLeft, ChevronRight, Clipboard, Download, Ellipsis, FileText, Link2, ShieldCheck, X } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { objectsApi } from '../api/objects'
import type { VerifiedVersion } from '../api/versions'
import { inspectorObjectKey, inspectorTabs, metadataHeaders, metadataMatches, metadataUpdate, readMetadataDraft, tagsMatch, type InspectorTab, type MetadataDraft } from '../model/inspector'
import { objectId, type ObjectDetails, type ObjectId, type ObjectVersion, type Tag } from '../model/objects'
import ObjectAccess from './ObjectAccess.vue'
import ObjectPreview from './ObjectPreview.vue'
import TagEditor from './TagEditor.vue'
import VersionList from './VersionList.vue'

const props = defineProps<{
  connection: string; revision?: string; object: ObjectId; deleted?: boolean; demo: boolean;
  initialTab?: InspectorTab; hasPrevious?: boolean; hasNext?: boolean;
  metadataDrafts?: ReadonlyMap<string, MetadataDraft>; tagDrafts?: ReadonlyMap<string, Tag[]>;
}>()
const emit = defineEmits<{
  close: []; move: [direction: -1 | 1]; download: [object: ObjectId]; changed: []; remove: [object: ObjectId];
  versionChanged: [state: VerifiedVersion];
  metadataDraft: [key: string, draft: MetadataDraft | undefined]; tagDraft: [key: string, draft: Tag[] | undefined];
}>()

const tab = ref<InspectorTab>('Preview')
const version = ref<ObjectVersion>()
const confirmedVersion = ref<VerifiedVersion>()
const details = ref<ObjectDetails>()
const tags = ref<Tag[]>()
const detailsError = ref('')
const error = ref('')
const copied = ref('')
const busy = ref(false)
const readingTags = ref(false)
const saving = ref(false)
const signing = ref(false)
const metadata = ref<MetadataDraft>()
const tagDraft = ref<Tag[]>()
const pendingExit = ref<() => void>()
const minutes = ref(30)
const share = ref<{ url: string; expiresAt: string }>()
const more = ref(false)
const closeButton = ref<HTMLButtonElement>()
let generation = 0

const target = computed(() => {
  if (version.value) return objectId(props.object.bucket, version.value.key, version.value.versionId)
  if (props.deleted && confirmedVersion.value) {
    const latest = confirmedVersion.value.latest
    return objectId(props.object.bucket, props.object.key, latest?.deleteMarker ? latest.versionId : null)
  }
  return objectId(props.object.bucket, props.object.key, props.object.versionId)
})
const deleted = computed(() => !version.value && props.deleted && (!confirmedVersion.value || !confirmedVersion.value.latest || confirmedVersion.value.latest.deleteMarker))
const allowedTabs = computed(() => deleted.value ? ['Versions'] as const : inspectorTabs)
const targetKey = computed(() => inspectorObjectKey(props.connection, props.revision, target.value))
const inspectorView = computed(() => JSON.stringify([targetKey.value, tab.value]))
const name = computed(() => target.value.key.split('/').at(-1) || target.value.key)
const dirty = computed(() => metadata.value?.dirty || tagDraft.value !== undefined)
const extension = computed(() => {
  const lastDot = name.value.lastIndexOf('.')
  return lastDot > 0 ? name.value.slice(lastDot + 1).slice(0, 10).toUpperCase() : 'OBJECT'
})

watch(() => [props.connection, props.revision, props.object.bucket, props.object.key, props.object.versionId, props.deleted, props.initialTab], () => {
  version.value = undefined
  confirmedVersion.value = undefined
  pendingExit.value = undefined
  tab.value = props.deleted ? 'Versions' : props.initialTab ?? 'Preview'
}, { immediate: true })

watch(targetKey, key => {
  generation++
  more.value = false
  share.value = undefined
  copied.value = ''
  error.value = ''
  detailsError.value = ''
  details.value = undefined
  tags.value = undefined
  saving.value = false
  signing.value = false
  busy.value = false
  readingTags.value = false
  metadata.value = props.metadataDrafts?.get(key)
  tagDraft.value = props.tagDrafts?.get(key)
  if (!deleted.value && metadata.value) tab.value = 'Details'
  else if (!deleted.value && tagDraft.value) tab.value = 'Tags'
}, { immediate: true })

watch([targetKey, tab], () => {
  if ((tab.value === 'Details' || tab.value === 'Access') && !details.value) void loadDetails()
  if (tab.value === 'Tags' && !tags.value) void loadTags()
}, { immediate: true })

onUnmounted(() => { generation++ })
defineExpose({ requestExit, focusHeader, openTab })

function focusHeader(): void {
  closeButton.value?.focus({ preventScroll: true })
}

function requestExit(action: () => void): void {
  if (saving.value) return
  if (dirty.value) pendingExit.value = action
  else {
    setMetadataDraft(undefined)
    action()
  }
}

function discardDraft(): void {
  const action = pendingExit.value
  pendingExit.value = undefined
  setMetadataDraft(undefined)
  setTagDraft(undefined)
  error.value = ''
  action?.()
}

function openTab(next: InspectorTab): void {
  if (tab.value === next || !(allowedTabs.value as readonly InspectorTab[]).includes(next)) return
  requestExit(() => {
    error.value = ''
    copied.value = ''
    tab.value = next
  })
}

function viewVersion(item: ObjectVersion): void {
  requestExit(() => {
    version.value = item
    tab.value = 'Preview'
  })
}

function backToObject(): void {
  requestExit(() => {
    version.value = undefined
    tab.value = 'Versions'
  })
}

async function loadDetails(): Promise<void> {
  if (busy.value) return
  const id = generation
  busy.value = true
  detailsError.value = ''
  try {
    const result = await objectsApi.details(props.connection, target.value)
    if (id === generation) details.value = result
  } catch (failure) {
    if (id === generation) detailsError.value = errorMessage(failure)
  } finally {
    if (id === generation) busy.value = false
  }
}

async function loadTags(): Promise<void> {
  if (readingTags.value) return
  const id = generation
  readingTags.value = true
  error.value = ''
  try {
    const result = await objectsApi.tags(props.connection, target.value)
    if (id === generation) tags.value = result
  } catch (failure) {
    if (id === generation) error.value = errorMessage(failure)
  } finally {
    if (id === generation) readingTags.value = false
  }
}

async function copy(value: string, message: string): Promise<void> {
  const id = generation
  try {
    await navigator.clipboard.writeText(value)
    if (id === generation) copied.value = message
  } catch (failure) {
    if (id === generation) error.value = errorMessage(failure)
  }
}

async function generateUrl(): Promise<void> {
  const expiry = minutes.value
  if (!Number.isInteger(expiry) || expiry < 1 || expiry > 10_080) {
    error.value = 'Enter an expiry between 1 minute and 7 days.'
    return
  }
  const id = generation
  const expiresAt = new Date(Date.now() + expiry * 60_000).toISOString()
  share.value = undefined
  error.value = ''
  signing.value = true
  try {
    const url = await objectsApi.share(props.connection, target.value, expiry)
    if (id === generation) share.value = { url, expiresAt }
  } catch (failure) {
    if (id === generation) error.value = errorMessage(failure)
  } finally {
    if (id === generation) signing.value = false
  }
}

function setMetadataDraft(draft: MetadataDraft | undefined): void {
  metadata.value = draft
  emit('metadataDraft', targetKey.value, draft?.dirty ? draft : undefined)
}

function beginMetadata(): void {
  if (details.value) setMetadataDraft(readMetadataDraft(details.value))
}

function updateHeader(key: keyof MetadataDraft['headers'], event: Event): void {
  if (!metadata.value || !(event.target instanceof HTMLInputElement)) return
  setMetadataDraft({ ...metadata.value, headers: { ...metadata.value.headers, [key]: event.target.value }, dirty: true })
}

function updateMetadata(event: Event): void {
  if (!metadata.value || !(event.target instanceof HTMLTextAreaElement)) return
  setMetadataDraft({ ...metadata.value, raw: event.target.value, dirty: true })
}

function setTagDraft(draft: Tag[] | undefined): void {
  tagDraft.value = draft
  emit('tagDraft', targetKey.value, draft)
}

async function commitReadback(action: (connection: string, object: ObjectId, current: () => boolean) => Promise<void>): Promise<void> {
  if (saving.value) return
  const id = generation
  const current = () => id === generation
  error.value = ''
  saving.value = true
  try {
    await action(props.connection, target.value, current)
  } catch (failure) {
    if (current()) error.value = errorMessage(failure)
  } finally {
    if (current()) saving.value = false
  }
}

async function saveMetadata(): Promise<void> {
  const draft = metadata.value
  if (!draft) return
  await commitReadback(async (connection, object, current) => {
    const update = metadataUpdate(draft)
    await objectsApi.change(connection, object, { kind: 'setMetadata', update })
    const state = await objectsApi.details(connection, object)
    if (!current()) return
    details.value = state
    if (!metadataMatches(update, state)) throw new Error('The provider returned different metadata or content headers. Your draft is still available.')
    setMetadataDraft(undefined)
    emit('changed')
  })
}

async function saveTags(values: Tag[]): Promise<void> {
  const update = values.map(item => ({ ...item }))
  await commitReadback(async (connection, object, current) => {
    await objectsApi.change(connection, object, { kind: 'setTags', tags: update })
    const state = await objectsApi.tags(connection, object)
    if (!current()) return
    if (!tagsMatch(update, state)) throw new Error('The provider returned different tags. Your draft is still available.')
    setTagDraft(undefined)
    tags.value = state
    emit('changed')
  })
}

function versionChanged(state: VerifiedVersion): void {
  if (state.key === props.object.key) {
    if (version.value && state.removedVersionId === version.value.versionId) version.value = undefined
    confirmedVersion.value = state
    details.value = undefined
  }
  emit('versionChanged', state)
}

function accessChanged(state: ObjectDetails | undefined): void {
  if (state) details.value = state
  emit('changed')
}
</script>

<template>
  <aside class="inspector" aria-label="Object inspector" @keydown.esc.stop="emit('close')">
    <header class="inspector-top">
      <strong>Object inspector</strong>
      <div class="inspector-navigation">
        <button class="icon-button subtle" aria-label="Previous object" :disabled="!hasPrevious" @click="emit('move', -1)"><ChevronLeft :size="17" /></button>
        <button class="icon-button subtle" aria-label="Next object" :disabled="!hasNext" @click="emit('move', 1)"><ChevronRight :size="17" /></button>
        <button ref="closeButton" class="icon-button subtle" aria-label="Close inspector" @click="emit('close')"><X :size="17" /></button>
      </div>
    </header>
    <nav class="inspector-tabs" aria-label="Inspector sections">
      <button v-for="name in allowedTabs" :key="name" :aria-pressed="tab === name" :class="{ active: tab === name }" @click="openTab(name)">{{ name }}</button>
    </nav>
    <div class="inspector-content">
      <header class="inspector-object-header">
        <h2 class="object-heading">{{ name }}</h2>
        <p v-if="deleted && target.versionId" class="muted">Current delete marker <code>{{ target.versionId }}</code></p>
        <p v-else-if="target.versionId" class="muted">Version <code>{{ target.versionId }}</code></p>
        <div class="panel-actions object-actions">
          <button class="primary" :disabled="demo || deleted" @click="emit('download', target)"><Download :size="16" /> Download</button>
          <button class="icon-button" title="Copy exact object key" aria-label="Copy exact object key" @click="copy(target.key, 'Object key copied.')"><Clipboard :size="16" /></button>
          <button class="icon-button" title="More actions" aria-label="More object actions" :aria-expanded="more" @click="more = !more"><Ellipsis :size="18" /></button>
        </div>
        <button v-if="version" class="subtle back-to-version" @click="backToObject">Back to object versions</button>
        <FadeRegion>
          <div v-if="more" class="panel-form object-more">
            <label>Signed URL expiry (minutes)<input v-model.number="minutes" type="number" min="1" max="10080" :disabled="demo || deleted" /></label>
            <button :disabled="demo || deleted || signing" @click="generateUrl"><Link2 :size="16" /> Generate signed URL</button>
            <label v-if="share">Signed URL<input :value="share.url" readonly @focus="($event.target as HTMLInputElement).select()" /></label>
            <p v-if="share" class="muted">Expires {{ formatDate(share.expiresAt, true) }}</p>
            <button v-if="share" @click="copy(share.url, 'Signed URL copied.')">Copy signed URL</button>
            <button class="danger-outline" :disabled="demo || deleted || !!target.versionId" @click="requestExit(() => emit('remove', target))">Delete object</button>
          </div>
        </FadeRegion>
      </header>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="copied" tone="success">{{ copied }}</Notice>
      <FadeRegion>
        <div :key="inspectorView">
          <ObjectPreview v-if="tab === 'Preview'" :connection="connection" :object="target" />
          <template v-else-if="tab === 'Details'">
            <figure class="document-tile">
              <div class="document-glyph"><FileText :size="38" :stroke-width="1.25" /></div>
              <figcaption>{{ extension }}</figcaption>
            </figure>
            <p v-if="busy" class="loading-line" role="status">Reading object details…</p>
            <Notice v-if="detailsError" tone="error">{{ detailsError }}</Notice>
            <button v-if="detailsError" class="subtle" @click="loadDetails">Retry details</button>
            <dl class="object-properties">
              <div><dt>Object key</dt><dd><code>{{ target.key }}</code></dd></div>
              <div v-if="details"><dt>Size</dt><dd>{{ formatSize(details.size) }}</dd></div>
              <div v-if="details"><dt>Storage class</dt><dd><span class="chip">{{ details.storageClass || 'STANDARD' }}</span></dd></div>
              <div v-if="details"><dt>Last modified</dt><dd>{{ formatDate(details.modified, true) }}</dd></div>
              <div v-if="details?.etag"><dt>ETag</dt><dd><code>{{ details.etag }}</code></dd></div>
              <div v-if="details?.versionId"><dt>Version ID</dt><dd><code>{{ details.versionId }}</code></dd></div>
              <div v-if="details?.encryption"><dt>Encryption</dt><dd class="inline"><ShieldCheck :size="16" />{{ details.encryption }}</dd></div>
              <div v-if="details?.retainUntil"><dt>Retain until</dt><dd>{{ formatDate(details.retainUntil, true) }}</dd></div>
              <div v-if="details?.restore"><dt>Restore status</dt><dd>{{ details.restore }}</dd></div>
              <template v-if="details">
                <div v-for="header in metadataHeaders.filter(header => details?.[header.key])" :key="header.key"><dt>{{ header.label }}</dt><dd>{{ details[header.key] }}</dd></div>
              </template>
            </dl>
            <section v-if="details || metadata" class="metadata-section">
              <div class="inline between"><h3>Metadata</h3><button v-if="!metadata" class="subtle" :disabled="demo || !!target.versionId" @click="beginMetadata">Edit metadata</button></div>
              <FadeRegion>
                <form v-if="metadata" class="panel-form" @submit.prevent="saveMetadata">
                  <label v-for="header in metadataHeaders" :key="header.key">{{ header.label }}<input :value="metadata.headers[header.key]" :disabled="saving" @input="updateHeader(header.key, $event)" /></label>
                  <label>User metadata JSON<textarea :value="metadata.raw" :disabled="saving" rows="6" spellcheck="false" @input="updateMetadata" /></label>
                  <p class="muted">Saving replaces metadata on the current object and may create a new version.</p>
                  <div class="panel-actions"><button type="button" :disabled="saving" @click="requestExit(() => setMetadataDraft(undefined))">Cancel</button><button class="primary" :disabled="saving || !metadata.dirty">Save metadata</button></div>
                </form>
                <dl v-else class="metadata-list"><div v-for="(value, key) in details?.metadata" :key="key"><dt>{{ key }}</dt><dd>{{ value }}</dd></div></dl>
              </FadeRegion>
            </section>
          </template>
          <VersionList v-else-if="tab === 'Versions'" :connection="connection" :bucket="object.bucket" :prefix="object.key" exact :demo="demo"
            @download="emit('download', $event)" @view="viewVersion" @changed="versionChanged" />
          <template v-else-if="tab === 'Tags'">
            <TagEditor v-if="tags" :tags="tags" :initial-draft="tagDraft" :disabled="demo || saving" @save="saveTags" @draft="setTagDraft" />
            <button v-else-if="error" class="subtle" @click="loadTags">Retry tags</button>
            <p v-else class="loading-line" role="status">Reading tags…</p>
          </template>
          <template v-else-if="tab === 'Access'">
            <Notice v-if="detailsError" tone="error">{{ detailsError }}</Notice>
            <button class="subtle" :disabled="busy" @click="loadDetails">{{ busy ? 'Reading details…' : 'Refresh details' }}</button>
            <ObjectAccess :connection="connection" :object="target" :details="details" :demo="demo" @changed="accessChanged" />
          </template>
        </div>
      </FadeRegion>
    </div>
    <Modal v-if="pendingExit" title="Discard object edits?" @close="pendingExit = undefined">
      <div class="modal-form">
        <p class="muted">The edits for this object are still in your draft.</p>
        <div class="modal-actions">
          <button @click="pendingExit = undefined">Keep editing</button>
          <button class="danger" @click="discardDraft">Discard draft</button>
        </div>
      </div>
    </Modal>
  </aside>
</template>

<style scoped>
.inspector-navigation { display: flex; gap: 3px; }
.inspector-object-header { margin-bottom: 11px; }
.inspector-object-header p { margin-top: 5px; overflow-wrap: anywhere; }
.inspector-object-header code, .object-properties code { white-space: pre-wrap; }
.back-to-version { margin-bottom: 4px; padding-inline: 0; }
</style>
