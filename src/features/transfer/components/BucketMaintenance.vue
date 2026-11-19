<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import type { Bucket } from '../../bucket/model/bucket'
import type { Location } from '../../browser/model/objects'
import { transfersApi } from '../api/transfers'
import { scopeLabel, type BucketResult, type BucketScope, type BucketSelection, type BucketStatus, type FailureStage, type IntegrityMethod, type MaintenanceFailure, type MaintenanceKind, type PurgePreview } from '../model/maintenance'
import { useJobs } from '../store/jobs'

const props = defineProps<{ kind: MaintenanceKind; location: Location; sourceBuckets: Bucket[]; demo: boolean }>()
const jobs = useJobs()
const selected = ref<BucketScope[]>(initialTargets())
const filter = ref('')
const parallelism = ref(4)
const method = ref<IntegrityMethod['kind']>('head')
const sampleBytes = ref(65536)
const since = ref('')
const includeVersions = ref(false)
const removeBucket = ref(false)
const review = ref<PurgePreview>()
const confirm = ref(false)
const busy = ref(false)
const error = ref('')
const jobId = ref('')
const failureText = ref('')
const failureStage = ref<FailureStage | 'all'>('all')
let generation = 0

const methodOptions: { value: IntegrityMethod['kind']; label: string }[] = [
  { value: 'head', label: 'HEAD metadata' }, { value: 'sample', label: 'GET byte sample' }, { value: 'full', label: 'GET full object' },
]
const stageLabels: Record<FailureStage, string> = {
  listing: 'Listing', review: 'Preview validation', filter: 'Modification time', head: 'HEAD', get: 'GET', delete: 'Delete object', deleteBucket: 'Delete bucket',
}
const failureOptions: { value: FailureStage | 'all'; label: string }[] = [
  { value: 'all', label: 'All stages' },
  { value: 'listing', label: stageLabels.listing },
  { value: 'review', label: stageLabels.review },
  { value: 'filter', label: stageLabels.filter },
  { value: 'head', label: stageLabels.head },
  { value: 'get', label: stageLabels.get },
  { value: 'delete', label: stageLabels.delete },
  { value: 'deleteBucket', label: stageLabels.deleteBucket },
]
const statusLabels: Record<BucketStatus, string> = {
  pending: 'Queued', listing: 'Listing', running: 'Running', completed: 'Completed', failed: 'Failed', cancelled: 'Cancelled',
}
const available = computed(() => props.sourceBuckets.filter(bucket =>
  bucket.name.toLocaleLowerCase().includes(filter.value.trim().toLocaleLowerCase())))
const allowedRemoval = computed(() => includeVersions.value && selected.value.every(scope => !scope.prefix))
const valid = computed(() => selected.value.length > 0 && wholeNumber(parallelism.value, 1, 32) &&
  (props.kind === 'purge' || method.value !== 'sample' || wholeNumber(sampleBytes.value, 1, 16777216)))
const job = computed(() => jobs.jobs.find(candidate =>
  (!jobId.value || candidate.id === jobId.value) && candidate.maintenance?.kind === props.kind &&
  candidate.maintenance.connection === props.location.connection))
const jobRunning = computed(() => job.value?.status === 'running')
const disabled = computed(() => props.demo || jobRunning.value || busy.value)
const failedBuckets = computed(() => job.value?.maintenance?.buckets.filter(bucket => bucket.status === 'failed') ?? [])
const operationErrors = computed(() => job.value?.failures.filter(failure => failure.item === 'Operation') ?? [])

watch(() => props.location.connection, () => {
  ++generation
  jobId.value = ''
  busy.value = false
  selected.value = initialTargets()
})
watch([selected, parallelism, includeVersions, removeBucket], () => {
  error.value = ''
  review.value = undefined
  confirm.value = false
}, { flush: 'sync' })
watch(allowedRemoval, allowed => {
  if (!allowed) removeBucket.value = false
}, { flush: 'sync' })

function initialTargets(): BucketScope[] {
  return props.location.bucket ? [{ bucket: props.location.bucket, prefix: props.location.prefix }] : []
}

function wholeNumber(value: number, min: number, max: number): boolean {
  return Number.isInteger(value) && value >= min && value <= max
}

function targetFor(bucket: string): BucketScope | undefined {
  return selected.value.find(scope => scope.bucket === bucket)
}

function setSelected(bucket: string, enabled: boolean): void {
  if (!enabled) {
    selected.value = selected.value.filter(scope => scope.bucket !== bucket)
  } else if (!targetFor(bucket)) {
    selected.value = [...selected.value, scopeFor(bucket)]
  }
}

function setVisible(enabled: boolean): void {
  if (enabled) {
    const existing = new Set(selected.value.map(scope => scope.bucket))
    const added = available.value.filter(bucket => !existing.has(bucket.name)).map(bucket => scopeFor(bucket.name))
    selected.value = [...selected.value, ...added]
  } else {
    const visible = new Set(available.value.map(bucket => bucket.name))
    selected.value = selected.value.filter(scope => !visible.has(scope.bucket))
  }
}

function scopeFor(bucket: string): BucketScope {
  return { bucket, prefix: bucket === props.location.bucket ? props.location.prefix : '' }
}

function clearPrefix(bucket: string): void {
  selected.value = selected.value.map(scope => scope.bucket === bucket ? { ...scope, prefix: '' } : scope)
}

function selection(): BucketSelection {
  return { connection: props.location.connection, parallelism: parallelism.value, targets: selected.value.map(scope => ({ ...scope })) }
}

async function run(): Promise<void> {
  if (disabled.value || !valid.value) return
  const id = ++generation
  busy.value = true
  error.value = ''
  try {
    if (props.kind === 'purge') {
      const preview = await transfersApi.previewPurge({
        selection: selection(), includeVersions: includeVersions.value, removeBucket: removeBucket.value,
      })
      if (id === generation) review.value = preview
    } else {
      const check: IntegrityMethod = method.value === 'sample' ? { kind: 'sample', bytes: sampleBytes.value } : { kind: method.value }
      const job = await transfersApi.integrity({
        selection: selection(), includeVersions: includeVersions.value, method: check,
        since: since.value ? new Date(since.value).toISOString() : null,
      })
      await jobs.load()
      if (id === generation) jobId.value = job
    }
  } catch (failure) {
    if (id === generation) error.value = errorMessage(failure)
  } finally {
    if (id === generation) busy.value = false
  }
}

async function purge(): Promise<void> {
  const preview = review.value
  if (!preview?.confirmation || disabled.value) return
  confirm.value = false
  busy.value = true
  error.value = ''
  const id = ++generation
  try {
    const job = await jobs.start({ kind: 'purge', preview, confirmation: preview.confirmation })
    if (id === generation) {
      review.value = undefined
      jobId.value = job
    }
  } catch (failure) {
    if (id === generation) error.value = errorMessage(failure)
  } finally {
    if (id === generation) busy.value = false
  }
}

async function cancel(): Promise<void> {
  if (!job.value) return
  try {
    await jobs.cancel(job.value.id)
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}

function filterFailures(bucket: BucketResult): MaintenanceFailure[] {
  const text = failureText.value.toLocaleLowerCase()
  return bucket.failures.filter(failure =>
    (failureStage.value === 'all' || failure.stage === failureStage.value) &&
    [failure.key, failure.version, failure.message].some(field => field?.toLocaleLowerCase().includes(text)))
}

function retryFailed(): void {
  const settings = job.value?.maintenance
  if (!settings) return
  selected.value = failedBuckets.value.map(({ scope }) => ({ ...scope }))
  parallelism.value = settings.parallelism
  includeVersions.value = settings.includeVersions
  if (settings.kind === 'purge') {
    removeBucket.value = settings.removeBucket
  } else {
    method.value = settings.method.kind
    if (settings.method.kind === 'sample') sampleBytes.value = settings.method.bytes
    since.value = settings.since ? localDateTime(settings.since) : ''
  }
  void run()
}

function localDateTime(value: string): string {
  const instant = new Date(value)
  return new Date(instant.getTime() - instant.getTimezoneOffset() * 60000).toISOString().slice(0, 16)
}

function configurationOutcome(bucket: BucketResult): string {
  if (bucket.removed) return 'Deleted'
  return bucket.failures.some(failure => failure.stage === 'deleteBucket') ? 'Unverified' : 'Preserved'
}
</script>

<template>
  <div class="bucket-maintenance">
    <form class="panel-form" @submit.prevent="run">
      <div class="stage-heading">
        <div>
          <h2>{{ kind === 'integrity' ? 'Bucket integrity' : 'Bucket purge' }}</h2>
          <p>{{ kind === 'integrity' ? 'Check that objects and versions can be read.' : 'Review the objects and versions to remove from selected buckets.' }}</p>
        </div>
      </div>
      <fieldset class="bucket-fieldset">
        <legend>Buckets</legend>
        <div class="bucket-selection-tools">
          <input v-model="filter" type="search" placeholder="Filter buckets" aria-label="Filter buckets" />
          <button type="button" class="subtle" :disabled="disabled || !available.length" @click="setVisible(true)">Select visible</button>
          <button type="button" class="subtle" :disabled="disabled || !available.length" @click="setVisible(false)">Clear visible</button>
          <span class="muted">{{ selected.length }} selected</span>
        </div>
        <div class="bucket-selection">
          <div v-for="bucket in available" :key="bucket.name" class="bucket-choice">
            <label class="check-label">
              <input type="checkbox" :checked="!!targetFor(bucket.name)" :disabled="disabled" @change="setSelected(bucket.name, ($event.target as HTMLInputElement).checked)" />
              {{ bucket.name }}
            </label>
            <template v-if="targetFor(bucket.name)?.prefix">
              <code>{{ targetFor(bucket.name)?.prefix }}</code>
              <button type="button" class="subtle" :disabled="disabled" @click="clearPrefix(bucket.name)">Use whole bucket</button>
            </template>
          </div>
          <p v-if="!available.length" class="muted">No buckets match.</p>
        </div>
      </fieldset>

      <div class="form-grid">
        <label>Parallel requests per bucket<input v-model.number="parallelism" type="number" min="1" max="32" :disabled="disabled" required /></label>
        <template v-if="kind === 'integrity'">
          <label>Verification method<AppSelect v-model="method" :options="methodOptions" :disabled="disabled" /></label>
          <label v-if="method === 'sample'">Sample bytes<input v-model.number="sampleBytes" type="number" min="1" max="16777216" :disabled="disabled" required /></label>
          <label>Modified since <small>Optional</small><input v-model="since" type="datetime-local" :disabled="disabled" /></label>
        </template>
      </div>
      <label class="check-label"><input v-model="includeVersions" type="checkbox" :disabled="disabled" />
        {{ kind === 'integrity' ? 'Check all object versions' : 'Permanently remove all versions and delete markers' }}
      </label>
      <label v-if="kind === 'purge' && allowedRemoval" class="check-label">
        <input v-model="removeBucket" type="checkbox" :disabled="disabled" /> Delete each bucket after a successful purge
      </label>
      <div class="panel-actions">
        <button class="primary" :disabled="disabled || !valid">{{ busy ? 'Working…' : kind === 'integrity' ? 'Run integrity check' : 'Preview purge' }}</button>
      </div>
    </form>

    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <template v-if="review">
      <section class="maintenance-results" aria-label="Purge preview">
        <h3>Purge preview</h3>
        <p class="muted">{{ review.request.selection.parallelism }} parallel requests per bucket ·
          {{ review.request.removeBucket ? 'Delete eligible buckets after emptying' : 'Preserve bucket configuration' }}</p>
        <div class="maintenance-table">
          <table class="report-table">
            <thead><tr><th>Bucket or prefix</th><th>Current objects</th><th>History</th><th>Delete markers</th><th>Scope</th></tr></thead>
            <tbody>
              <tr v-for="bucket in review.buckets" :key="bucket.scope.bucket">
                <th scope="row"><code>{{ scopeLabel(bucket.scope) }}</code></th>
                <td>{{ bucket.counts?.current ?? 'Unknown' }}</td>
                <td>{{ bucket.historyError ? 'Unknown' : bucket.counts?.history ?? 'Unknown' }}</td>
                <td>{{ bucket.historyError ? 'Unknown' : bucket.counts?.markers ?? 'Unknown' }}</td>
                <td>{{ bucket.error || (review.request.includeVersions ? 'All versions and markers' : 'Current objects') }}
                  <p v-if="bucket.historyError" class="muted">{{ bucket.historyError }}</p>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <button type="button" class="danger-outline" :disabled="!review.confirmation || disabled" @click="confirm = true">Review purge confirmation</button>
      </section>
      <ConfirmDialog v-if="confirm" :title="review.request.removeBucket ? 'Empty and delete reviewed buckets?' : 'Purge reviewed buckets?'"
        :description="review.request.includeVersions ? 'All reviewed object versions and delete markers will be permanently removed.' : 'The current objects in the reviewed scope will be deleted.'"
        :action="review.request.removeBucket ? 'Empty and delete buckets' : 'Purge selected buckets'" :phrase="review.confirmation" danger
        @close="confirm = false" @confirm="purge" />
    </template>

    <section v-if="job && job.maintenance" class="maintenance-results" aria-label="Bucket job results">
      <h3>{{ kind === 'integrity' ? 'Integrity results' : 'Purge results' }}</h3>
      <div class="bucket-selection-tools">
        <span aria-live="polite">{{ job.succeeded + job.failed }} / {{ job.total }} buckets processed · {{ job.status }}</span>
        <button v-if="jobRunning" type="button" :disabled="job.cancelRequested" @click="cancel">{{ job.cancelRequested ? 'Cancelling…' : 'Cancel job' }}</button>
        <button v-else-if="failedBuckets.length" type="button" :disabled="disabled" @click="retryFailed">{{ kind === 'purge' ? 'Preview failed buckets' : 'Retry failed buckets' }}</button>
      </div>
      <Notice v-for="issue in operationErrors" :key="issue.message" tone="error">{{ issue.message }}</Notice>
      <div v-if="job.failed || job.maintenance.buckets.some(bucket => bucket.failed)" class="form-grid">
        <label>Filter failures<input v-model="failureText" type="search" placeholder="Key, version or error" /></label>
        <label>Error stage<AppSelect v-model="failureStage" :options="failureOptions" /></label>
      </div>
      <details v-for="bucket in job.maintenance.buckets" :key="bucket.scope.bucket" class="bucket-outcome">
        <summary><strong><code>{{ scopeLabel(bucket.scope) }}</code></strong>
          <span>{{ jobRunning || bucket.status !== 'pending' ? statusLabels[bucket.status] : 'Not processed' }}</span>
          <span>{{ bucket.failed }} failures</span>
        </summary>
        <dl class="result-totals">
          <div><dt>Current objects</dt><dd>{{ bucket.completed.current }} / {{ bucket.planned.current }}</dd></div>
          <div><dt>Historical versions</dt><dd>{{ bucket.completed.history }} / {{ bucket.planned.history }}</dd></div>
          <div v-if="kind === 'purge'"><dt>Delete markers</dt><dd>{{ bucket.completed.markers }} / {{ bucket.planned.markers }}</dd></div>
          <div v-if="kind === 'purge' && (bucket.status === 'completed' || bucket.status === 'failed')"><dt>Bucket configuration</dt><dd>{{ configurationOutcome(bucket) }}</dd></div>
        </dl>
        <p v-if="bucket.failed > bucket.failures.length" class="muted">{{ bucket.failed - bucket.failures.length }} additional failures omitted.</p>
        <ul v-if="filterFailures(bucket).length" class="failure-list">
          <li v-for="(failure, index) in filterFailures(bucket)" :key="index">
            <strong>{{ stageLabels[failure.stage] }}</strong>
            <code v-if="failure.key">{{ JSON.stringify(failure.key) }}</code>
            <span v-if="failure.version">Version {{ failure.version }}</span>
            <span>{{ failure.message }}</span>
          </li>
        </ul>
        <p v-else-if="bucket.failed" class="muted">No failures match the filter.</p>
      </details>
    </section>
  </div>
</template>

<style scoped>
.bucket-maintenance, .bucket-fieldset, .maintenance-results { display: grid; gap: 10px; min-width: 0; }
.bucket-fieldset { margin: 0; padding: 0; border: 0; }
.bucket-fieldset legend { padding: 0; margin-bottom: 7px; font-weight: 650; }
.bucket-selection-tools, .bucket-choice { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.bucket-selection-tools > input { width: min(15rem, 100%); }
.bucket-selection { max-height: 12rem; overflow: auto; border: 1px solid var(--line); }
.bucket-choice { padding: 6px 9px; border-bottom: 1px solid var(--line); }
.bucket-choice:last-child { border-bottom: 0; }
.bucket-selection > p { padding: 8px; }
.bucket-choice code, .bucket-outcome code, td code { white-space: pre-wrap; overflow-wrap: anywhere; }
.maintenance-results { margin-top: 8px; }
.maintenance-results > :is(h3, p) { margin: 0; }
.maintenance-results > button { justify-self: start; }
.maintenance-table { overflow-x: auto; }
.bucket-outcome { border-bottom: 1px solid var(--line); padding-block: 7px; }
.bucket-outcome summary { cursor: pointer; padding-block: 4px; }
.bucket-outcome summary > span { margin-inline-start: 12px; color: var(--muted); }
</style>
