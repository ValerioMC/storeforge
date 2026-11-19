<script setup lang="ts">
import { onMounted, ref, shallowRef, watch } from 'vue'
import { formatDate } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { objectsApi } from '../../browser/api/objects'
import type { Location, MultipartUpload, ObjectVersion } from '../../browser/model/objects'
import { transfersApi } from '../api/transfers'
import { toTimestamp, versionCsv, type VersionPlanItem, type VersionRule } from '../model/batch'

const props = defineProps<{ location: Location; submitting: boolean; jobError: string }>()
const emit = defineEmits<{ close: []; job: [rule: VersionRule, items: VersionPlanItem[]] }>()
const tabs = ['History', 'Restore', 'Cleanup', 'Multipart'] as const
const view = ref<(typeof tabs)[number]>('History')
const history = shallowRef<ObjectVersion[]>()
const uploads = shallowRef<MultipartUpload[]>()
const aborting = ref<MultipartUpload>()
const deleteMissing = ref(false)
const restoreMode = ref<'deleted' | 'dated'>('deleted')
const targetDate = ref('')
const keep = ref<number | ''>('')
const cutoff = ref('')
const orphanMarkers = ref(false)
const preview = shallowRef<{ items: VersionPlanItem[]; rule: VersionRule }>()
const busy = ref(false)
const failure = ref('')
const limit = ref(100)
const actionLabels = { restore: 'Restore version', markDeleted: 'Mark deleted', deleteVersion: 'Delete version' }

onMounted(() => void loadHistory())
watch([restoreMode, targetDate, deleteMissing, keep, cutoff, orphanMarkers], () => { preview.value = undefined })
watch(view, next => {
  failure.value = ''
  limit.value = 100
  preview.value = undefined
  aborting.value = undefined
  if (next === 'History' && !history.value) void loadHistory()
  if (next === 'Multipart' && !uploads.value) void loadUploads()
})

async function run(action: () => Promise<void>): Promise<void> {
  busy.value = true
  failure.value = ''
  try {
    await action()
  } catch (error) {
    failure.value = errorMessage(error)
  } finally {
    busy.value = false
  }
}

async function loadHistory(): Promise<void> {
  await run(async () => { history.value = await transfersApi.history(props.location) })
}

async function loadUploads(): Promise<void> {
  await run(async () => { uploads.value = await objectsApi.multipart(props.location) })
}

async function exportHistory(): Promise<void> {
  const rows = history.value
  if (rows) await run(() => transfersApi.exportReport('prefix-versions.csv', versionCsv(rows)))
}

function versionRule(): VersionRule {
  if (view.value === 'Restore') {
    return restoreMode.value === 'deleted' ? { kind: 'restoreDeleted' } : {
      kind: 'restoreAt', at: toTimestamp(targetDate.value), deleteMissing: deleteMissing.value,
    }
  }
  if (!keep.value && !cutoff.value && !orphanMarkers.value) throw new Error('Set a keep count, an age cutoff, or orphan marker cleanup.')
  if (keep.value !== '' && (!Number.isSafeInteger(keep.value) || keep.value < 1)) throw new Error('Enter a positive whole number of versions to keep.')
  return { kind: 'cleanup', keep: keep.value || null, before: cutoff.value ? toTimestamp(cutoff.value) : null, orphanMarkers: orphanMarkers.value }
}

async function previewChanges(): Promise<void> {
  preview.value = undefined
  limit.value = 100
  await run(async () => {
    const rule = versionRule()
    const items = await transfersApi.versionPlan(props.location, rule)
    preview.value = { rule, items }
  })
}

function applyChanges(): void {
  if (preview.value?.items.length) emit('job', preview.value.rule, preview.value.items)
}

async function abortUpload(): Promise<void> {
  const upload = aborting.value
  if (!upload) return
  await run(async () => {
    const { connection, bucket } = props.location
    await objectsApi.abortMultipart(connection, bucket, upload.key, upload.uploadId)
    uploads.value = await objectsApi.multipart(props.location)
    if (uploads.value.some(item => item.key === upload.key && item.uploadId === upload.uploadId)) {
      throw new Error('The provider still lists this multipart upload.')
    }
    aborting.value = undefined
  })
}
</script>

<template>
  <Modal title="Prefix operations" wide @close="emit('close')">
    <div class="modal-form prefix-operations">
      <code class="exact-key">s3://{{ location.bucket }}/{{ location.prefix }}</code>
      <nav class="workspace-tabs" aria-label="Prefix operations">
        <button v-for="tab in tabs" :key="tab" :aria-pressed="view === tab" :class="{ active: view === tab }" :disabled="busy"
          @click="view = tab">{{ tab }}</button>
      </nav>
      <Notice v-if="failure || jobError" tone="error">{{ failure || jobError }}</Notice>
      <p v-if="busy" role="status">Contacting the provider…</p>
      <template v-if="view === 'History'">
        <div class="inline between">
          <strong>{{ history?.length ?? 0 }} versions and delete markers</strong>
          <button :disabled="busy || !history?.length" @click="exportHistory">Export CSV</button>
        </div>
        <div v-if="history?.length" class="batch-table">
          <table>
            <thead><tr><th scope="col">Key</th><th scope="col">Version</th><th scope="col">State</th><th scope="col">Modified</th></tr></thead>
            <tbody>
              <tr v-for="row in history.slice(0, limit)" :key="JSON.stringify([row.key, row.versionId])">
                <td><code class="exact-key">{{ row.key }}</code></td><td><code class="exact-key">{{ row.versionId }}</code></td>
                <td>{{ row.latest ? 'Current' : 'Previous' }}{{ row.deleteMarker ? ' marker' : ' object' }}</td>
                <td>{{ formatDate(row.modified, true) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <button v-if="history && limit < history.length" @click="limit += 100">Show more versions</button>
        <button v-if="!history && !busy" @click="loadHistory">Retry history</button>
      </template>
      <template v-else-if="view === 'Multipart'">
        <p v-if="uploads && !uploads.length" class="muted">No incomplete uploads under this prefix.</p>
        <div v-for="upload in uploads?.slice(0, limit)" :key="JSON.stringify([upload.key, upload.uploadId])" class="multipart-entry">
          <div><code class="exact-key">{{ upload.key }}</code><small><code class="exact-key">{{ upload.uploadId }}</code> · {{ formatDate(upload.initiated, true) }}</small></div>
          <button class="danger-outline" :disabled="busy || !!aborting" @click="aborting = upload">Abort</button>
        </div>
        <button v-if="uploads && limit < uploads.length" @click="limit += 100">Show more uploads</button>
        <button v-if="!uploads && !busy" @click="loadUploads">Retry multipart listing</button>
        <div v-if="aborting" class="modal-form settings-sheet">
          <p>Discard incomplete parts for <code class="exact-key">{{ aborting.key }}</code>?</p>
          <div class="panel-actions"><button :disabled="busy" @click="aborting = undefined">Keep upload</button>
            <button class="danger" :disabled="busy" @click="abortUpload">Abort upload</button>
          </div>
        </div>
      </template>
      <form v-else class="panel-form" @submit.prevent="previewChanges">
        <template v-if="view === 'Restore'">
          <label>Restore mode<AppSelect v-model="restoreMode" :disabled="busy"
            :options="[{ value: 'deleted', label: 'Latest version of deleted objects' }, { value: 'dated', label: 'State at a date and time' }]" /></label>
          <template v-if="restoreMode === 'dated'">
            <label>Restore point<input v-model="targetDate" type="datetime-local" required :disabled="busy" /></label>
            <label class="check"><input v-model="deleteMissing" type="checkbox" :disabled="busy" /> Mark objects missing at that point as deleted</label>
          </template>
        </template>
        <template v-else>
          <label>Keep latest versions per key<input v-model="keep" type="number" min="1" step="1" :disabled="busy" /></label>
          <label>Remove versions older than<input v-model="cutoff" type="datetime-local" :disabled="busy" /></label>
          <label class="check"><input v-model="orphanMarkers" type="checkbox" :disabled="busy" /> Remove older orphan delete markers</label>
          <p class="muted">A historical version is eligible if either rule matches. The current version is always kept.</p>
        </template>
        <div class="panel-actions"><button :disabled="busy">Preview changes</button></div>
        <section v-if="preview" aria-label="Reviewed changes" class="panel-form">
          <strong>{{ preview.items.length }} proposed changes</strong>
          <div v-if="preview.items.length" class="batch-table">
            <table>
              <thead><tr><th scope="col">Key</th><th scope="col">Version</th><th scope="col">Action</th></tr></thead>
              <tbody>
                <tr v-for="item in preview.items.slice(0, limit)" :key="JSON.stringify([item.key, item.versionId])">
                  <td><code class="exact-key">{{ item.key }}</code></td><td><code class="exact-key">{{ item.versionId }}</code></td><td>{{ actionLabels[item.action] }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <button v-if="limit < preview.items.length" type="button" @click="limit += 100">Show more changes</button>
          <div v-if="preview.items.length" class="panel-actions">
            <button type="button" :class="view === 'Cleanup' || deleteMissing ? 'danger' : 'primary'" :disabled="busy || submitting"
              @click="applyChanges">Apply {{ preview.items.length }} reviewed {{ preview.items.length === 1 ? 'change' : 'changes' }}</button>
          </div>
        </section>
      </form>
    </div>
  </Modal>
</template>

<style scoped>
.exact-key { white-space: pre-wrap; overflow-wrap: anywhere; }
.prefix-operations label.check { display: flex; align-items: center; gap: 7px; }
.prefix-operations .workspace-tabs { margin: 0; }
.batch-table { max-height: 250px; overflow: auto; border: 1px solid var(--line); border-radius: var(--radius); }
.batch-table table { width: 100%; border-collapse: collapse; font-size: 12px; }
.batch-table th, .batch-table td { padding: 7px 9px; border-bottom: 1px solid var(--line); text-align: left; }
.batch-table th { position: sticky; top: 0; background: var(--surface); }
.multipart-entry { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding-block: 7px; border-bottom: 1px solid var(--line); }
.multipart-entry small { display: block; }
</style>
