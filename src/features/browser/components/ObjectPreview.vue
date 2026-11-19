<script setup lang="ts">
import { computed, defineAsyncComponent, onUnmounted, ref, shallowRef, watch } from 'vue'
import { FileQuestion } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Notice from '../../../components/ui/Notice.vue'
import { objectsApi } from '../api/objects'
import type { ObjectId, Preview } from '../model/objects'
import { decodePreview, decodePreviewText, previewText, searchPreview, type TextPreview } from '../model/preview'

const PdfPreview = defineAsyncComponent(() => import('./PdfPreview.vue'))
const props = defineProps<{ connection: string; object: ObjectId }>()
const error = ref('')
const busy = ref(false)
const text = ref<TextPreview>()
const url = ref<string>()
const pdf = shallowRef<Uint8Array<ArrayBuffer>>()
const kind = ref<Preview['kind']>()
const truncated = ref(false)
const oversized = ref(false)
const raw = ref(false)
const query = ref('')
const matches = computed(() => searchPreview(text.value?.raw ?? '', query.value))
const matchCount = computed(() => matches.value.filter(part => part.match).length)
let request = 0

watch(() => [props.connection, props.object.bucket, props.object.key, props.object.versionId], () => void load(), { immediate: true })
onUnmounted(() => {
  request++
  clear()
})

function clear(): void {
  if (url.value) URL.revokeObjectURL(url.value)
  url.value = undefined
  text.value = undefined
  pdf.value = undefined
  kind.value = undefined
  truncated.value = false
  oversized.value = false
  raw.value = false
  query.value = ''
}

function display(preview: Preview): void {
  oversized.value = preview.oversized
  if (preview.oversized) return
  truncated.value = preview.truncated
  const bytes = decodePreview(preview)
  kind.value = preview.kind
  if (preview.kind === 'text') {
    const value = decodePreviewText(bytes, preview.truncated)
    if (value !== undefined) text.value = previewText(preview.contentType, value, preview.truncated)
  } else if (preview.kind === 'pdf') {
    pdf.value = bytes
  } else if (preview.contentType.startsWith(preview.kind + '/')) {
    url.value = URL.createObjectURL(new Blob([bytes], { type: preview.contentType }))
  }
}

function mediaUnavailable(): void {
  error.value = 'This media could not be played or displayed. Download the object to open it on your device.'
}

async function load(): Promise<void> {
  const id = ++request
  busy.value = true
  error.value = ''
  clear()
  try {
    const preview = await objectsApi.preview(props.connection, props.object)
    if (id === request) display(preview)
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request) busy.value = false
  }
}
</script>

<template>
  <p class="preview-limits muted">Preview limits: 50 MiB per object, 64 KiB of text.</p>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <p v-else-if="busy" class="loading-line" role="status">Loading preview…</p>
  <PdfPreview v-else-if="pdf" :data="pdf" />
  <img v-else-if="url && kind === 'image'" class="image-preview" :src="url" :alt="'Preview of ' + object.key" @error="mediaUnavailable" />
  <audio v-else-if="url && kind === 'audio'" controls preload="metadata" :src="url" :aria-label="'Audio preview of ' + object.key" @error="mediaUnavailable" />
  <video v-else-if="url && kind === 'video'" controls preload="metadata" :src="url" :aria-label="'Video preview of ' + object.key" @error="mediaUnavailable" />
  <div v-else-if="text" class="preview-text">
    <label>Find in preview<input v-model="query" type="search" /></label>
    <p v-if="query" class="muted" role="status">{{ matchCount }} {{ matchCount === 1 ? 'match' : 'matches' }}</p>
    <button v-else-if="text.structured" class="subtle" :aria-pressed="raw" @click="raw = !raw">{{ raw ? 'Show structured preview' : 'Show raw text' }}</button>
    <div v-if="!raw && !query && text.structured?.kind === 'csv'" class="preview-csv">
      <table aria-label="CSV preview">
        <thead><tr><th v-for="(cell, index) in text.structured.rows[0]?.slice(0, 24)" :key="index" scope="col">{{ cell }}</th></tr></thead>
        <tbody><tr v-for="(row, index) in text.structured.rows.slice(1, 151)" :key="index"><td v-for="(cell, column) in row.slice(0, 24)" :key="column">{{ cell }}</td></tr></tbody>
      </table>
      <p v-if="text.structured.rows.length > 151 || text.structured.rows.some(row => row.length > 24)" class="muted">Table shows up to 150 rows and 24 columns. View raw text for the full preview.</p>
    </div>
    <pre v-else-if="!raw && !query && text.structured?.kind === 'json'" class="text-preview" aria-label="Formatted JSON preview">{{ text.structured.formatted }}</pre>
    <pre v-else class="text-preview"><template v-for="(part, index) in matches" :key="index"><mark v-if="part.match">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template>{{ text.raw ? '' : 'Empty file' }}</pre>
    <p v-if="truncated" class="muted">Showing the first 64 KiB.</p>
  </div>
  <EmptyState v-else :icon="FileQuestion" title="Preview unavailable" :description="oversized ? 'This object exceeds 50 MiB. Download it to open it on your device.' : 'Download the object to open it on your device.'" />
</template>

<style scoped>
.preview-limits { margin-bottom: 9px; font-size: 12.5px; }
.preview-text { display: grid; gap: 6px; }
.preview-text label { display: grid; gap: 4px; }
.preview-text button { justify-self: start; }
.preview-csv { overflow: auto; max-height: 310px; border: 1px solid var(--line); border-radius: var(--radius); }
.preview-csv table { border-collapse: collapse; min-width: 100%; text-align: start; }
.preview-csv th, .preview-csv td { white-space: pre-wrap; padding: 5px 8px; border-bottom: 1px solid var(--line); }
.preview-csv th { background: var(--surface-alt); }
audio, video { width: 100%; }
</style>
