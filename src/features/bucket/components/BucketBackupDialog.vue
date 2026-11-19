<script setup lang="ts">
import { computed, onMounted, ref, shallowRef } from 'vue'
import { errorMessage } from '../../../common/tauri'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { bucketsApi } from '../api/buckets'
import type { useBucketConfiguration } from '../composables/useBucketConfiguration'
import type { Configuration } from '../model/bucket'
import { configurations } from '../model/configuration'
import { readBackup, readConfiguration, scanConfigurations, type BucketBackup, type ConfigurationReading } from '../model/workspace'

const props = defineProps<{ connection: string; bucket: string; mode: 'export' | 'restore'; editors: ReturnType<typeof useBucketConfiguration> }>()
const emit = defineEmits<{ close: []; changed: [] }>()
const backup = shallowRef<BucketBackup>()
const selection = ref<Configuration[]>([])
const readings = ref<Partial<Record<Configuration, ConfigurationReading>>>({})
const errors = ref<Partial<Record<Configuration, string>>>({})
const written = ref<Configuration[]>([])
const busy = ref(true)
const error = ref('')
const exporting = computed(() => props.mode === 'export')
const sections = computed(() => configurations.filter(({ id }) => exporting.value || backup.value?.documents[id] !== undefined))

onMounted(load)

async function load(): Promise<void> {
  busy.value = true
  error.value = ''
  try {
    if (!exporting.value) {
      const source = await bucketsApi.importBackup()
      if (source === null) {
        emit('close')
        return
      }
      backup.value = readBackup(source)
    }
    await scanConfigurations([props.bucket], sections.value.map(({ id }) => id),
      (bucket, section) => bucketsApi.configure(props.connection, bucket, section, { action: 'read' }),
      (_, section, reading) => { readings.value[section] = reading }, () => true)
    selection.value = sections.value.filter(({ id }) => eligible(id) && (exporting.value || backup.value?.bucket === props.bucket)).map(({ id }) => id)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}

function currentDocument(section: Configuration): string {
  const reading = readings.value[section]
  return reading && reading.status !== 'error' ? reading.document : ''
}

function reason(section: Configuration): string {
  const reading = readings.value[section]
  if (reading?.status === 'error') return reading.message
  if (written.value.includes(section)) return 'Restored'
  if (exporting.value) return currentDocument(section) ? '' : 'No provider document'
  const local = props.editors.state(section)
  if (local.draft !== local.original) return 'Discard or save the local draft first'
  return currentDocument(section) === backup.value?.documents[section] ? 'Matches current configuration' : ''
}

function eligible(section: Configuration): boolean {
  return !!readings.value[section] && !reason(section)
}

async function submit(): Promise<void> {
  const selected = selection.value.filter(eligible)
  if (busy.value || !selected.length) return
  busy.value = true
  error.value = ''
  try {
    if (exporting.value) {
      const document: BucketBackup = {
        format: 'storeforge-bucket-config', version: 1, bucket: props.bucket,
        documents: Object.fromEntries(selected.map(section => [section, currentDocument(section)])),
      }
      if (await bucketsApi.exportBackup(props.bucket, JSON.stringify(document, null, 2))) emit('close')
      return
    }
    await restore(selected)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}

async function restore(selected: Configuration[]): Promise<void> {
  for (const section of selected) {
    const document = backup.value?.documents[section]
    if (!document) continue
    const latest = await readCurrent(section)
    if (latest.status === 'error' || latest.document !== currentDocument(section)) {
      errors.value[section] = latest.status === 'error' ? latest.message : 'The provider configuration changed. Review the current document before retrying.'
      readings.value[section] = latest
      selection.value = selection.value.filter(item => item !== section)
      continue
    }
    props.editors.edit(section, document)
    const saved = await props.editors.write(section, { action: 'write', document })
    if (saved) {
      written.value.push(section)
      emit('changed')
    } else {
      errors.value[section] = props.editors.state(section).error
    }
  }
}

async function readCurrent(section: Configuration): Promise<ConfigurationReading> {
  return readConfiguration((bucket, configuration) => bucketsApi.configure(props.connection, bucket, configuration, { action: 'read' }), props.bucket, section)
}
</script>

<template>
  <Modal :title="exporting ? 'Back up bucket configuration' : 'Review bucket configuration restore'" wide @close="!busy && emit('close')">
    <form class="modal-form" @submit.prevent="submit">
      <p v-if="exporting" class="muted">Export provider documents from {{ bucket }}. Local drafts stay in the editor.</p>
      <p v-else-if="backup" class="muted">Restore selected documents from <strong>{{ backup.bucket }}</strong> to <strong>{{ bucket }}</strong> on the active connection.</p>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <p v-if="busy" role="status" class="muted">{{ exporting ? 'Reading or saving provider documents…' : 'Reading or restoring configuration…' }}</p>
      <div v-if="exporting || backup" class="bucket-backup-list">
        <div v-for="{ id, label } in sections" :key="id">
          <label class="check-label"><input v-model="selection" type="checkbox" :value="id" :disabled="busy || !eligible(id)" /> {{ label }}</label>
          <p v-if="reason(id)" class="muted">{{ reason(id) }}</p>
          <Notice v-if="errors[id]" tone="error">{{ errors[id] }}</Notice>
          <details v-if="!exporting">
            <summary>Inspect documents</summary>
            <h3>Current</h3><pre>{{ currentDocument(id) || 'No provider document' }}</pre>
            <h3>Backup</h3><pre>{{ backup?.documents[id] }}</pre>
          </details>
        </div>
      </div>
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="emit('close')">{{ written.length ? 'Done' : 'Close' }}</button>
        <button :class="exporting ? 'primary' : 'danger'" :disabled="busy || !selection.some(eligible)">{{ exporting ? 'Save backup' : 'Restore selected' }}</button>
      </div>
    </form>
  </Modal>
</template>

<style scoped>
.bucket-backup-list { max-height: 51vh; overflow: auto; }
.bucket-backup-list > div { padding: 8px 0; border-bottom: 1px solid var(--line); }
.bucket-backup-list p, .bucket-backup-list details { margin-inline-start: 24px; }
pre { max-height: 180px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; background: var(--surface-alt); padding: 8px; }
</style>
