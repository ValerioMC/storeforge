<script setup lang="ts">
import { computed, ref, shallowRef, watch } from 'vue'
import { Braces, Database, Download, RefreshCw, Trash2, Upload } from 'lucide-vue-next'
import { errorMessage, isMissing } from '../../../common/tauri'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import { bucketsApi } from '../api/buckets'
import type { Configuration, ConfigurationChange } from '../model/bucket'
import { configurations } from '../model/configuration'
import { readPolicy } from '../model/policy-document'
import type { JsonObject, JsonValue } from '../model/policy-document'
import { readXml, writeXml } from '../model/xml-document'
import { xmlSettings } from '../model/xml-settings'
import PolicySettingsForm from './PolicySettingsForm.vue'
import XmlSettingsForm from './XmlSettingsForm.vue'

const props = defineProps<{ connection: string; bucket: string; demo: boolean }>()
const emit = defineEmits<{ removed: [] }>()
const configuration = ref<Configuration>('versioning')
const current = computed(() => configurations.find(item => item.id === configuration.value) ?? configurations[0]!)
const schema = computed(() => configuration.value === 'policy' ? undefined : xmlSettings[configuration.value])
const xml = shallowRef<XMLDocument>()
const policy = shallowRef<JsonObject>()
const revision = ref(0)
const document = ref('')
const original = ref('')
const busy = ref(false)
const exporting = ref(false)
const error = ref('')
const notice = ref<{ text: string; tone: 'info' | 'success' }>()
const confirm = ref<'write' | 'remove' | 'bucket'>()
const fileInput = ref<HTMLInputElement>()
let request = 0

watch(() => [props.connection, props.bucket, configuration.value], () => void load(), { immediate: true })

function setDraft(source: string): void {
  if (schema.value) xml.value = readXml(source, schema.value.root)
  else policy.value = readPolicy(source)
  document.value = source
  revision.value++
}

async function load(): Promise<void> {
  const id = ++request
  busy.value = true
  error.value = ''
  notice.value = undefined
  confirm.value = undefined
  xml.value = undefined
  policy.value = undefined
  document.value = ''
  original.value = ''
  try {
    const response = props.demo ? current.value.example : await bucketsApi.configure(props.connection, props.bucket, configuration.value, { action: 'read' })
    if (id !== request) return
    original.value = response
    setDraft(response)
  } catch (failure) {
    if (id !== request) return
    setDraft('')
    if (isMissing(failure)) notice.value = { text: 'This bucket has no saved configuration for this setting.', tone: 'info' }
    else error.value = errorMessage(failure)
  } finally {
    if (id === request) busy.value = false
  }
}

function updateXml(): void {
  if (!xml.value) return
  document.value = writeXml(xml.value.documentElement)
  revision.value++
}

function updatePolicy(value: JsonValue): void {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return
  policy.value = value
  document.value = JSON.stringify(value, null, 2)
}

function loadExample(): void {
  setDraft(current.value.example)
  error.value = ''
  notice.value = undefined
}

async function importFile(event: Event): Promise<void> {
  const input = event.currentTarget as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  const id = ++request
  busy.value = true
  try {
    const source = await file.text()
    if (id !== request) return
    if (!source.trim()) throw new Error('The configuration file is empty.')
    setDraft(source)
    error.value = ''
    notice.value = { text: 'Configuration imported. Apply settings to save it to the bucket.', tone: 'info' }
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request) busy.value = false
  }
}

async function exportFile(): Promise<void> {
  exporting.value = true
  const id = request
  try {
    if (await bucketsApi.exportConfiguration(props.bucket, configuration.value, current.value.format, document.value) && id === request) {
      notice.value = { text: 'Configuration exported.', tone: 'success' }
    }
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    exporting.value = false
  }
}

function review(): void {
  try {
    if (schema.value) readXml(document.value, schema.value.root)
    else readPolicy(document.value)
    confirm.value = 'write'
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}

async function apply(): Promise<void> {
  const action = confirm.value
  confirm.value = undefined
  if (!action) return
  const id = ++request
  busy.value = true
  error.value = ''
  notice.value = undefined
  try {
    if (action === 'bucket') {
      await bucketsApi.remove(props.connection, props.bucket)
      if (id === request) emit('removed')
      return
    }
    const change: ConfigurationChange = action === 'write' ? { action, document: document.value } : { action }
    await bucketsApi.configure(props.connection, props.bucket, configuration.value, change)
    if (id !== request) return
    const reload = load()
    const reloadId = request
    await reload
    if (reloadId === request && !error.value) notice.value = { text: 'Bucket settings updated.', tone: 'success' }
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request) busy.value = false
  }
}
</script>

<template>
  <div class="stage-panel configuration-panel">
    <div class="stage-heading"><div><h2>Bucket settings</h2><p>Read and change the settings supported by this endpoint.</p></div><Database :size="20" /></div>
    <div class="configuration-grid">
      <nav aria-label="Bucket settings" class="settings-nav">
        <button v-for="item in configurations" :key="item.id" :aria-pressed="configuration === item.id" :class="{ active: configuration === item.id }" @click="configuration = item.id">{{ item.label }}</button>
      </nav>
      <div class="panel-form configuration-editor">
        <div class="settings-toolbar">
          <h3>{{ current.label }}</h3>
          <div class="panel-actions">
            <input ref="fileInput" type="file" hidden :accept="`.${current.format.toLowerCase()}`" @change="importFile" />
            <button type="button" :disabled="busy || demo" @click="fileInput?.click()"><Upload :size="15" /> Import {{ current.format }}</button>
            <button type="button" :disabled="busy || exporting || !document" @click="exportFile"><Download :size="15" /> Export {{ current.format }}</button>
            <button class="icon-button" type="button" aria-label="Reload configuration" :disabled="busy" @click="load"><RefreshCw :size="17" /></button>
          </div>
        </div>
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <Notice v-if="notice" :tone="notice.tone">{{ notice.text }}</Notice>
        <p v-if="busy && !xml && !policy" class="muted" role="status">Reading endpoint configuration…</p>
        <form class="panel-form" :aria-busy="busy" @submit.prevent="review">
          <XmlSettingsForm v-if="schema && xml" :key="configuration" :parent="xml.documentElement"
            :fields="schema.fields" :revision="revision" :disabled="busy || demo" @change="updateXml" />
          <PolicySettingsForm v-else-if="policy" :value="policy" scope="policy" :disabled="busy || demo" @change="updatePolicy" />
          <div class="panel-actions configuration-actions">
            <button v-if="current.example" type="button" :disabled="busy || demo" @click="loadExample"><Braces :size="15" /> Load example</button>
            <button v-if="current.remove" type="button" :disabled="busy || demo || !original" @click="confirm = 'remove'">Remove setting</button>
            <button class="primary" type="submit" :disabled="busy || demo || !document || document === original">Apply settings</button>
          </div>
        </form>
      </div>
    </div>
    <div class="danger-zone"><div><h3>Delete empty bucket</h3><p>Remove the bucket after all objects and versions have been removed.</p></div>
      <button :disabled="demo || busy" class="danger-outline" @click="confirm = 'bucket'"><Trash2 :size="16" /> Delete bucket</button>
    </div>
    <ConfirmDialog v-if="confirm" :title="confirm === 'write' ? `Update ${current.label.toLowerCase()}?` : confirm === 'bucket' ? 'Delete bucket?' : 'Remove configuration?'"
      :description="confirm === 'bucket' ? `${bucket} will be removed from the selected endpoint.` : `This applies to the ${bucket} bucket on the selected connection.`"
      :action="confirm === 'write' ? 'Apply settings' : confirm === 'bucket' ? 'Delete bucket' : 'Remove setting'"
      :phrase="confirm === 'bucket' ? bucket : undefined" :danger="confirm !== 'write'" @close="confirm = undefined" @confirm="apply" />
  </div>
</template>
