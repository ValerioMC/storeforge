<script setup lang="ts">
import { computed, ref, shallowRef, watch } from 'vue'
import { Braces, Database, Download, RefreshCw, Trash2, Upload } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import { bucketsApi } from '../api/buckets'
import { useBucketConfiguration } from '../composables/useBucketConfiguration'
import type { Configuration, ConfigurationChange } from '../model/bucket'
import { configurations } from '../model/configuration'
import type { JsonValue } from '../model/policy-document'
import { configurationGroups, configurationLabel, extractRules, parseConfiguration, setLifecycleRuleStatus, unsupportedConfigurationMessages } from '../model/workspace'
import { addXmlField, setXmlValue, writeXml, xmlChildren } from '../model/xml-document'
import { xmlSettings } from '../model/xml-settings'
import BucketBackupDialog from './BucketBackupDialog.vue'
import PolicySettingsForm from './PolicySettingsForm.vue'
import XmlSettingsForm from './XmlSettingsForm.vue'

const props = defineProps<{ connection: string; bucket: string; demo: boolean; managed?: boolean; notificationTopic?: string; scopedRequest?: (section: Configuration, change: ConfigurationChange) => Promise<string> }>()
const emit = defineEmits<{ removed: []; changed: [] }>()
const configuration = ref<Configuration>(props.notificationTopic ? 'notification' : 'versioning')
const current = computed(() => configurations.find(item => item.id === configuration.value)!)
const schema = computed(() => configuration.value === 'policy' ? undefined : xmlSettings[configuration.value])
const editors = shallowRef(newEditors())
const state = computed(() => editors.value.state(configuration.value))
const readUnsupported = computed(() => state.value.reading?.status === 'unsupported')
const xml = shallowRef<XMLDocument>()
const policy = shallowRef<JsonValue>()
const revision = ref(0)
const operationError = ref('')
const operationNotice = ref('')
const exporting = ref(false)
const importing = ref(false)
const backup = ref<'export' | 'restore'>()
const confirm = ref<'write' | 'remove' | 'bucket'>()
const fileInput = ref<HTMLInputElement>()
const lifecycleRules = computed(() => configuration.value === 'lifecycle' ? extractRules('lifecycle', state.value.draft) : [])
const dirty = computed(() => editors.value.dirty.value)
const busy = computed(() => editors.value.writing.value || importing.value || exporting.value || !!backup.value)
let scope = 0

watch(() => [props.connection, props.bucket, props.demo], () => {
  scope++
  editors.value = newEditors()
  configuration.value = props.notificationTopic ? 'notification' : 'versioning'
  confirm.value = undefined
  backup.value = undefined
  importing.value = false
  exporting.value = false
}, { flush: 'sync' })

watch(() => props.notificationTopic, topic => {
  if (topic) configuration.value = 'notification'
})

watch([editors, configuration], ([editor, section]) => {
  operationError.value = ''
  operationNotice.value = ''
  void editor.refresh(section)
}, { immediate: true })

watch([configuration, () => state.value.draft], ([section, draft]) => {
  const parsed = parseConfiguration(section, draft)
  xml.value = parsed.xml
  policy.value = parsed.policy
  revision.value++
}, { immediate: true })

function newEditors() {
  const { connection, bucket, demo, scopedRequest } = props
  return useBucketConfiguration((section, change) => {
    if (demo) return Promise.resolve(configurations.find(item => item.id === section)!.example)
    return scopedRequest ? scopedRequest(section, change) : bucketsApi.configure(connection, bucket, section, change)
  })
}

function labelFor(section: Configuration): string {
  const settings = editors.value.state(section)
  if (settings.writing) return 'Saving…'
  if (settings.busy) return 'Reading…'
  const status = settings.reading?.status === 'error' ? 'Read error' : settings.reading?.label
  return [status, settings.draft !== settings.original ? 'Draft' : ''].filter(Boolean).join(' · ')
}

function edit(source: string): void {
  parseConfiguration(configuration.value, source)
  editors.value.edit(configuration.value, source)
  operationNotice.value = ''
  operationError.value = ''
}

function updateXml(): void {
  if (xml.value) edit(writeXml(xml.value.documentElement))
}

function updatePolicy(value: JsonValue): void {
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) edit(JSON.stringify(value, null, 2))
}

function addLinkedTopic(): void {
  const root = xml.value?.documentElement
  const arn = props.notificationTopic
  const group = xmlSettings.notification.fields.find(field => field.name === 'TopicConfiguration')
  if (!root || !arn || group?.kind !== 'group') return
  const topic = group.fields.find(field => field.name === 'Topic')
  if (!topic || topic.kind === 'group') return
  addXmlField(root, group)
  const rule = xmlChildren(root, group.name).at(-1)
  if (!rule) return
  setXmlValue(rule, topic, arn)
  updateXml()
}

async function importFile(event: Event): Promise<void> {
  const input = event.currentTarget as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  const id = scope
  const section = configuration.value
  importing.value = true
  try {
    const source = await file.text()
    if (id !== scope || section !== configuration.value) return
    if (!source.trim()) throw new Error('The configuration file is empty.')
    edit(source)
    operationNotice.value = 'Configuration imported into the draft.'
  } catch (failure) {
    if (id === scope && section === configuration.value) operationError.value = errorMessage(failure)
  } finally {
    if (id === scope) importing.value = false
  }
}

async function exportFile(): Promise<void> {
  exporting.value = true
  const id = scope
  const section = configuration.value
  try {
    if (await bucketsApi.exportConfiguration(props.bucket, section, current.value.format, state.value.draft) &&
      id === scope && section === configuration.value) operationNotice.value = 'Configuration exported.'
  } catch (failure) {
    if (id === scope && section === configuration.value) operationError.value = errorMessage(failure)
  } finally {
    if (id === scope) exporting.value = false
  }
}

async function apply(): Promise<void> {
  const action = confirm.value
  confirm.value = undefined
  if (!action) return
  const id = scope
  const section = configuration.value
  if (action === 'bucket') {
    importing.value = true
    try {
      await bucketsApi.remove(props.connection, props.bucket)
      if (id === scope) {
        editors.value.discard()
        importing.value = false
        emit('removed')
      }
    } catch (failure) {
      if (id === scope) operationError.value = errorMessage(failure)
    } finally {
      if (id === scope) importing.value = false
    }
    return
  }
  const change = action === 'write' ? { action, document: state.value.draft } : { action }
  if (await editors.value.write(section, change) && id === scope) emit('changed')
}

function toggleRule(index: number, status: string): void {
  edit(setLifecycleRuleStatus(state.value.draft, index, status === 'Enabled' ? 'Disabled' : 'Enabled'))
}

defineExpose({
  dirty, busy,
  discard: () => editors.value.discard(),
  refresh: () => editors.value.refreshLoaded(),
})
</script>

<template>
  <div class="stage-panel configuration-panel bucket-configuration">
    <div class="stage-heading">
      <div><h2>Bucket settings</h2><p>{{ bucket }}</p></div>
      <div v-if="managed" class="panel-actions">
        <button type="button" :disabled="demo || busy" @click="backup = 'export'"><Download :size="15" /> Back up</button>
        <button type="button" :disabled="demo || busy" @click="backup = 'restore'"><Upload :size="15" /> Restore</button>
      </div>
      <Database v-else :size="20" />
    </div>
    <div class="configuration-grid">
      <nav aria-label="Bucket settings" class="settings-nav">
        <div v-for="group in configurationGroups" :key="group.label" class="bucket-settings-group">
          <h3>{{ group.label }}</h3>
          <button v-for="section in group.sections" :key="section" type="button" :aria-pressed="configuration === section"
            :class="{ active: configuration === section }" @click="configuration = section">
            <span>{{ configurationLabel(section) }}</span><small v-if="labelFor(section)">{{ labelFor(section) }}</small>
          </button>
        </div>
      </nav>
      <FadeRegion>
        <div :key="configuration" class="panel-form configuration-editor">
          <div class="settings-toolbar">
            <h3>{{ current.label }}</h3>
            <div class="panel-actions">
              <input ref="fileInput" type="file" hidden :accept="`.${current.format.toLowerCase()}`" @change="importFile" />
              <button type="button" :disabled="state.busy || importing || demo || readUnsupported" @click="fileInput?.click()"><Upload :size="15" /> Import {{ current.format }}</button>
              <button type="button" :disabled="state.busy || exporting || !state.draft" @click="exportFile"><Download :size="15" /> Export {{ current.format }}</button>
              <button class="icon-button" type="button" aria-label="Reload configuration" :disabled="state.busy" @click="editors.refresh(configuration, true)"><RefreshCw :size="17" /></button>
            </div>
          </div>
          <Notice v-if="state.reading?.status === 'error'" tone="error">Read failed. {{ state.reading.message }}</Notice>
          <Notice v-else-if="state.reading?.status === 'unsupported'">{{ state.reading.message }}</Notice>
          <Notice v-if="state.error || operationError" tone="error">{{ state.error || operationError }}</Notice>
          <Notice v-if="state.unsupportedAction">{{ unsupportedConfigurationMessages[state.unsupportedAction] }}</Notice>
          <Notice v-if="state.notice || operationNotice" tone="info">{{ state.notice || operationNotice }}</Notice>
          <p v-if="!state.reading && state.busy" class="muted" role="status">Reading endpoint configuration…</p>
          <div v-else-if="readUnsupported && state.draft !== state.original" class="panel-actions configuration-actions">
            <button type="button" :disabled="state.busy" @click="edit(state.original)">Discard draft</button>
          </div>
          <form v-else-if="!readUnsupported" class="panel-form" :aria-busy="state.busy" @submit.prevent="confirm = 'write'">
            <div v-if="managed && lifecycleRules.length" class="lifecycle-quick-actions">
              <div v-for="(rule, index) in lifecycleRules" :key="index">
                <span>{{ rule.name }} <small class="muted">{{ rule.status }}</small></span>
                <button v-if="rule.status === 'Enabled' || rule.status === 'Disabled'" type="button" :disabled="state.busy || demo"
                  @click="toggleRule(index, rule.status)">{{ rule.status === 'Enabled' ? 'Disable' : 'Enable' }}</button>
              </div>
            </div>
            <div v-if="configuration === 'notification' && notificationTopic" class="panel-actions">
              <button type="button" :disabled="!xml || state.busy || demo || state.reading?.status === 'error'" @click="addLinkedTopic">Add selected SNS topic to draft</button>
            </div>
            <XmlSettingsForm v-if="schema && xml" :parent="xml.documentElement" :fields="schema.fields"
              :revision="revision" :disabled="state.busy || demo" @change="updateXml" />
            <PolicySettingsForm v-else-if="policy" :value="policy" scope="policy" :disabled="state.busy || demo" @change="updatePolicy" />
            <div class="panel-actions configuration-actions">
              <button v-if="current.example" type="button" :disabled="state.busy || demo" @click="edit(current.example)"><Braces :size="15" /> Load example</button>
              <button v-if="state.draft !== state.original" type="button" :disabled="state.busy" @click="edit(state.original)">Discard draft</button>
              <button v-if="current.remove" type="button" :disabled="state.busy || demo || !state.original" @click="confirm = 'remove'">Remove setting</button>
              <button class="primary" type="submit" :disabled="state.busy || demo || !state.draft || state.draft === state.original || state.reading?.status === 'error'">Apply settings</button>
            </div>
          </form>
        </div>
      </FadeRegion>
    </div>
    <div v-if="!scopedRequest" class="danger-zone"><div><h3>Delete empty bucket</h3><p>Remove the bucket after all objects and versions have been removed.</p></div>
      <button :disabled="demo || busy" class="danger-outline" @click="confirm = 'bucket'"><Trash2 :size="16" /> Delete bucket</button>
    </div>
    <BucketBackupDialog v-if="backup" :mode="backup" :connection="connection" :bucket="bucket" :editors="editors" :scoped-request="scopedRequest"
      @close="backup = undefined" @changed="emit('changed')" />
    <ConfirmDialog v-if="confirm" :title="confirm === 'write' ? `Update ${current.label.toLowerCase()}?` : confirm === 'bucket' ? 'Delete bucket?' : 'Remove configuration?'"
      :description="confirm === 'bucket' ? `${bucket} will be removed from the selected endpoint.` : `This applies to the ${bucket} bucket on the selected connection.`"
      :action="confirm === 'write' ? 'Apply settings' : confirm === 'bucket' ? 'Delete bucket' : 'Remove setting'"
      :phrase="confirm === 'bucket' ? bucket : undefined" :danger="confirm !== 'write'" @close="confirm = undefined" @confirm="apply" />
  </div>
</template>
