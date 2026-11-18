<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Clipboard, Download, Ellipsis, FileText, Link2, ShieldCheck, X } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import Notice from '../../../components/ui/Notice.vue'
import { objectsApi } from '../api/objects'
import type { ObjectChange, ObjectDetails, ObjectId, Tag } from '../model/objects'
import ObjectAccess from './ObjectAccess.vue'
import ObjectPreview from './ObjectPreview.vue'
import TagEditor from './TagEditor.vue'
import VersionList from './VersionList.vue'

const props = defineProps<{ connection: string; object: ObjectId; demo: boolean }>()
const emit = defineEmits<{ close: []; download: [object: ObjectId]; changed: []; remove: [object: ObjectId] }>()
const tab = ref<'Details' | 'Preview' | 'Versions' | 'Tags' | 'Access'>('Details')
const tabs = ['Details', 'Preview', 'Versions', 'Tags', 'Access'] as const
const details = ref<ObjectDetails>()
const tags = ref<Tag[]>([])
const detailsError = ref('')
const error = ref('')
const copied = ref('')
const busy = ref(false)
const editing = ref(false)
const metadata = ref('')
const contentType = ref('')
const cacheControl = ref('')
const minutes = ref(30)
const shareUrl = ref('')
const more = ref(false)
let request = 0

const name = computed(() => props.object.key.split('/').at(-1) || props.object.key)
const extension = computed(() => {
  const lastDot = name.value.lastIndexOf('.')
  return lastDot > 0 ? name.value.slice(lastDot + 1).slice(0, 10).toUpperCase() : 'OBJECT'
})

watch(() => [props.connection, props.object.bucket, props.object.key, props.object.versionId], () => {
  tab.value = 'Details'
  editing.value = false
  more.value = false
  shareUrl.value = ''
  copied.value = ''
  error.value = ''
  details.value = undefined
  tags.value = []
  void load()
}, { immediate: true })

watch(tab, value => {
  error.value = ''
  if (value === 'Tags') void loadTags()
})

async function load(): Promise<void> {
  const id = ++request
  busy.value = true
  detailsError.value = ''
  try {
    const result = await objectsApi.details(props.connection, props.object)
    if (id === request) details.value = result
  } catch (failure) {
    if (id === request) detailsError.value = errorMessage(failure)
  } finally {
    if (id === request) busy.value = false
  }
}

async function loadTags(): Promise<void> {
  const id = request
  try {
    const result = await objectsApi.tags(props.connection, props.object)
    if (id === request) tags.value = result
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  }
}

async function copy(value: string, message: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(value)
    copied.value = message
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}

async function generateUrl(): Promise<void> {
  try {
    shareUrl.value = await objectsApi.share(props.connection, props.object, minutes.value)
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}

function editMetadata(): void {
  const current = details.value
  if (!current) return
  metadata.value = JSON.stringify(current.metadata, null, 2)
  contentType.value = current.contentType ?? ''
  cacheControl.value = current.cacheControl ?? ''
  editing.value = true
}

async function saveMetadata(): Promise<void> {
  try {
    const parsed: unknown = JSON.parse(metadata.value)
    if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object' || Object.values(parsed).some(value => typeof value !== 'string')) {
      throw new Error('Metadata must be a JSON object with string values.')
    }
    await change({ kind: 'setMetadata', update: { metadata: parsed as Record<string, string>, contentType: contentType.value || null, cacheControl: cacheControl.value || null } })
    editing.value = false
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}

async function saveTags(values: Tag[]): Promise<void> {
  try {
    await change({ kind: 'setTags', tags: values })
    await loadTags()
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}

async function change(update: ObjectChange): Promise<void> {
  error.value = ''
  await objectsApi.change(props.connection, props.object, update)
  emit('changed')
  await load()
}
</script>

<template>
  <aside class="inspector" aria-label="Object inspector">
    <header class="inspector-top"><strong>Object inspector</strong><button class="icon-button subtle" aria-label="Close inspector" @click="emit('close')"><X :size="17" /></button></header>
    <nav class="inspector-tabs" aria-label="Inspector sections">
      <button v-for="name in tabs" :key="name" :aria-pressed="tab === name" :class="{ active: tab === name }" @click="tab = name">{{ name }}</button>
    </nav>
    <div class="inspector-content">
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="copied" tone="success">{{ copied }}</Notice>
      <template v-if="tab === 'Details'">
        <figure class="document-tile">
          <div class="document-glyph"><FileText :size="38" :stroke-width="1.25" /></div>
          <figcaption>{{ extension }}</figcaption>
        </figure>
        <h2 class="object-heading">{{ name }}</h2>
        <p class="object-subtitle">{{ details?.contentType || (busy ? 'Reading object details…' : 'S3 object') }}</p>
        <div class="panel-actions object-actions">
          <button class="primary" :disabled="demo" title="Download object" @click="emit('download', object)"><Download :size="16" /> Download</button>
          <button class="icon-button" title="Copy exact object key" aria-label="Copy exact object key" @click="copy(object.key, 'Object key copied.')"><Clipboard :size="16" /></button>
          <button class="icon-button" title="More actions" aria-label="More object actions" :aria-expanded="more" @click="more = !more"><Ellipsis :size="18" /></button>
        </div>
        <div v-if="more" class="panel-form object-more">
          <label>Signed URL expiry (minutes)<input v-model.number="minutes" type="number" min="1" max="10080" :disabled="demo" /></label>
          <button :disabled="demo" @click="generateUrl"><Link2 :size="16" /> Generate signed URL</button>
          <label v-if="shareUrl">Signed URL<input :value="shareUrl" readonly @focus="($event.target as HTMLInputElement).select()" /></label>
          <button v-if="shareUrl" @click="copy(shareUrl, 'Signed URL copied.')">Copy signed URL</button>
          <button class="danger-outline" :disabled="demo" @click="emit('remove', object)">Delete object</button>
        </div>
        <Notice v-if="detailsError" tone="error">{{ detailsError }}</Notice>
        <dl class="object-properties">
          <div><dt>Object key</dt><dd><code>{{ object.key }}</code></dd></div>
          <div v-if="details" class="property-split"><section><dt>Size</dt><dd>{{ formatSize(details.size) }}</dd></section><section><dt>Storage class</dt><dd><span class="chip">{{ details.storageClass || 'STANDARD' }}</span></dd></section></div>
          <div v-if="details"><dt>Last modified</dt><dd>{{ formatDate(details.modified, true) }}</dd></div>
          <div v-if="details?.etag"><dt>ETag</dt><dd><code>{{ details.etag }}</code></dd></div>
          <div v-if="details?.versionId"><dt>Version ID</dt><dd><code>{{ details.versionId }}</code></dd></div>
          <div v-if="details?.encryption"><dt>Encryption</dt><dd class="inline"><ShieldCheck :size="16" /> {{ details.encryption }}</dd></div>
          <div v-if="details?.retainUntil"><dt>Retain until</dt><dd>{{ formatDate(details.retainUntil, true) }}</dd></div>
          <div v-if="details?.restore"><dt>Restore status</dt><dd>{{ details.restore }}</dd></div>
        </dl>
        <section v-if="details" class="metadata-section">
          <div class="inline between"><h3>Metadata</h3><button v-if="!editing" class="subtle" :disabled="demo" @click="editMetadata">Edit</button></div>
          <form v-if="editing" class="panel-form" @submit.prevent="saveMetadata">
            <label>Content type<input v-model="contentType" /></label>
            <label>Cache control<input v-model="cacheControl" /></label>
            <label>User metadata JSON<textarea v-model="metadata" rows="6" spellcheck="false" /></label>
            <p class="muted">Saving replaces metadata on the current object and may create a new version.</p>
            <div class="panel-actions"><button type="button" @click="editing = false">Cancel</button><button class="primary">Save metadata</button></div>
          </form>
          <dl v-else class="metadata-list"><div v-for="(value, key) in details.metadata" :key="key"><dt>{{ key }}</dt><dd>{{ value }}</dd></div></dl>
        </section>
      </template>
      <ObjectPreview v-else-if="tab === 'Preview'" :connection="connection" :object="object" />
      <VersionList v-else-if="tab === 'Versions'" :connection="connection" :bucket="object.bucket" :prefix="object.key" exact :demo="demo" @download="emit('download', $event)" @changed="emit('changed'); load()" />
      <TagEditor v-else-if="tab === 'Tags'" :tags="tags" :disabled="demo" @save="saveTags" />
      <ObjectAccess v-else-if="tab === 'Access'" :connection="connection" :object="object" :details="details" :demo="demo" @changed="load()" />
    </div>
  </aside>
</template>
