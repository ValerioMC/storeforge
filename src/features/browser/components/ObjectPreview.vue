<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { FileQuestion } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Notice from '../../../components/ui/Notice.vue'
import { objectsApi } from '../api/objects'
import type { ObjectId } from '../model/objects'

const props = defineProps<{ connection: string; object: ObjectId }>()
const error = ref('')
const busy = ref(false)
const text = ref<string>()
const url = ref<string>()
const limit = ref(false)
let request = 0
const empty = computed(() => !busy.value && !error.value && text.value === undefined && !url.value)

watch(() => [props.connection, props.object.bucket, props.object.key, props.object.versionId], () => void load(), { immediate: true })
onUnmounted(() => {
  request++
  clear()
})

function clear(): void {
  if (url.value) URL.revokeObjectURL(url.value)
  url.value = undefined
  text.value = undefined
  limit.value = false
}

async function load(): Promise<void> {
  const id = ++request
  busy.value = true
  error.value = ''
  clear()
  try {
    const preview = await objectsApi.preview(props.connection, props.object)
    if (id !== request) return
    const mime = preview.contentType.split(';')[0]?.toLowerCase() ?? ''
    const bytes = Uint8Array.from(atob(preview.data), character => character.charCodeAt(0))
    limit.value = preview.truncated
    if (mime.startsWith('text/') || /(?:json|xml|yaml)(?:$|\W)/.test(mime)) {
      text.value = new TextDecoder().decode(bytes)
    } else if (!preview.truncated && ['image/png', 'image/jpeg', 'image/webp', 'image/gif', 'image/avif'].includes(mime)) {
      url.value = URL.createObjectURL(new Blob([bytes], { type: mime }))
    }
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request) busy.value = false
  }
}
</script>

<template>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <p v-if="busy" class="loading-line" role="status">Loading preview…</p>
  <img v-else-if="url" class="image-preview" :src="url" :alt="`Preview of ${object.key}`" />
  <pre v-else-if="text !== undefined" class="text-preview">{{ text || 'Empty file' }}</pre>
  <EmptyState v-else-if="empty" :icon="FileQuestion" title="Preview unavailable" description="Download the object to open it on your device." />
  <p v-if="limit" class="muted">Preview limited to the first 2 MiB.</p>
</template>
