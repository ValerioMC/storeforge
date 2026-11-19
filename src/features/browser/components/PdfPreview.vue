<script setup lang="ts">
import { nextTick, ref, shallowRef, watch } from 'vue'
import { ChevronLeft, ChevronRight } from 'lucide-vue-next'
import { getDocument, GlobalWorkerOptions, type PDFDocumentProxy } from 'pdfjs-dist/legacy/build/pdf.mjs'
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url'
import { errorMessage } from '../../../common/tauri'
import Notice from '../../../components/ui/Notice.vue'

const props = defineProps<{ data: Uint8Array<ArrayBuffer> }>()
const pageNumber = ref(1)
const document = shallowRef<PDFDocumentProxy>()
const canvas = ref<HTMLCanvasElement>()
const loading = ref(false)
const error = ref('')
GlobalWorkerOptions.workerSrc = workerUrl

watch(() => props.data, async (data, _, cleanup) => {
  let current = true
  const task = getDocument({ data, useWasm: false, enableXfa: false, maxImageSize: 12_000_000, canvasMaxAreaInBytes: 48_000_000 })
  cleanup(() => {
    current = false
    void task.destroy()
  })
  document.value = undefined
  pageNumber.value = 1
  error.value = ''
  loading.value = true
  try {
    const result = await task.promise
    if (current) document.value = result
  } catch (failure) {
    if (current) error.value = errorMessage(failure)
  } finally {
    if (current) loading.value = false
  }
}, { immediate: true })

watch([document, pageNumber], async ([pdf, number], _, cleanup) => {
  if (!pdf) return
  let current = true
  let cancel: (() => void) | undefined
  cleanup(() => { current = false; cancel?.() })
  try {
    error.value = ''
    const page = await pdf.getPage(number)
    await nextTick()
    if (!current || !canvas.value) return
    const natural = page.getViewport({ scale: 1 })
    const viewport = page.getViewport({ scale: Math.min(2, 1450 / Math.max(natural.width, natural.height)) })
    canvas.value.width = Math.ceil(viewport.width)
    canvas.value.height = Math.ceil(viewport.height)
    const rendering = page.render({ canvas: canvas.value, viewport })
    cancel = () => rendering.cancel()
    await rendering.promise
    if (current) page.cleanup()
  } catch (failure) {
    if (current) error.value = errorMessage(failure)
  }
})
</script>

<template>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <p v-if="loading" role="status" class="loading-line">Opening PDF…</p>
  <div v-if="document" class="pdf-preview">
    <nav aria-label="PDF pages">
      <button class="icon-button subtle" aria-label="Previous PDF page" :disabled="pageNumber === 1" @click="pageNumber--"><ChevronLeft :size="16" /></button>
      <span aria-live="polite">{{ pageNumber }} / {{ document.numPages }}</span>
      <button class="icon-button subtle" aria-label="Next PDF page" :disabled="pageNumber === document.numPages" @click="pageNumber++"><ChevronRight :size="16" /></button>
    </nav>
    <canvas ref="canvas" role="img" :aria-label="'PDF page ' + pageNumber" />
  </div>
</template>

<style scoped>
.pdf-preview nav { display: flex; align-items: center; justify-content: center; gap: 6px; margin-bottom: 8px; }
canvas { display: block; width: 100%; height: auto; border: 1px solid var(--line); }
</style>
