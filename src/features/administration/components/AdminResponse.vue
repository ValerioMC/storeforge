<script setup lang="ts">
import { computed, ref } from 'vue'
import { Braces, Check, Copy, Eye, EyeOff } from 'lucide-vue-next'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { credentialResponse, responseFields } from '../model/response'

const props = defineProps<{ response: string; demo?: boolean }>()
const copied = ref(false)
const error = ref('')
const reveal = ref(false)
const sensitive = computed(() => credentialResponse.test(props.response))
const format = computed(() => props.response.trim().startsWith('<') ? 'XML' : 'JSON')
const fields = computed(() => responseFields(props.response))
const formatted = computed(() => {
  const value = props.response.trim()
  if (value.startsWith('{') || value.startsWith('[')) {
    try {
      return JSON.stringify(JSON.parse(value), null, 2)
    } catch {
      return value
    }
  }
  return value.replace(/>\s*</g, '>\n<')
})

async function copyResponse(): Promise<void> {
  try {
    await navigator.clipboard.writeText(formatted.value)
    copied.value = true
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}
</script>

<template>
  <div class="response-panel">
    <div class="response-header">
      <div class="inline"><strong>{{ demo ? 'Sample response' : 'Endpoint response' }}</strong><span v-if="formatted" class="chip">{{ format }}</span></div>
      <button v-if="formatted && (!sensitive || reveal)" class="icon-button subtle" aria-label="Copy endpoint response" @click="copyResponse"><Check v-if="copied" :size="16" /><Copy v-else :size="16" /></button>
    </div>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <div v-if="sensitive" class="inline response-sensitive">
      <span class="muted">This response includes credential fields.</span>
      <button :aria-pressed="reveal" @click="reveal = !reveal"><EyeOff v-if="reveal" :size="16" /><Eye v-else :size="16" />{{ reveal ? 'Hide' : 'Reveal' }}</button>
    </div>
    <template v-if="formatted && (!sensitive || reveal)">
      <table v-if="fields.length" class="report-table response-table">
        <thead><tr><th>Field</th><th>Value</th></tr></thead>
        <tbody><tr v-for="({ path, value }, index) in fields" :key="index"><td>{{ path }}</td><td><code>{{ value }}</code></td></tr></tbody>
      </table>
      <details v-if="fields.length" class="response-raw"><summary><Braces :size="15" /> View raw response</summary><pre>{{ formatted }}</pre></details>
      <pre v-else>{{ formatted }}</pre>
    </template>
    <p v-else-if="!formatted" class="muted">Request accepted. The endpoint returned an empty body.</p>
  </div>
</template>
