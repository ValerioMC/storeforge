<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ArrowRight, Plus, X } from 'lucide-vue-next'
import { formatSize } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import { bucketsApi } from '../../bucket/api/buckets'
import type { Bucket, Configuration } from '../../bucket/model/bucket'
import { configurations } from '../../bucket/model/configuration'
import type { Location } from '../../browser/model/objects'
import type { Connection } from '../../connection/model/connection'
import { workspaceAvailable } from '../../connection/model/workspace'
import { transfersApi } from '../api/transfers'
import type { BucketComparison, BucketPair, ConfigurationDifference, ConfigurationSnapshot, ContentMode, ObjectDifference, Remediation } from '../model/bucket-comparison'
import { useJobs } from '../store/jobs'

interface Mapping extends BucketPair { id: number }
interface Review { pair: BucketPair; action: Remediation; title: string; detail: string }

const props = defineProps<{ location: Location; profiles: Connection[]; sourceBuckets: Bucket[]; demo: boolean }>()
const emit = defineEmits<{ browse: [location: Location]; job: [id: string] }>()
const jobs = useJobs()
const modeOptions: { value: ContentMode; label: string }[] = [
  { value: 'none', label: 'Skip objects' },
  { value: 'size', label: 'Exact keys and sizes' },
  { value: 'etag', label: 'Sizes and ETag evidence' },
  { value: 'checksum', label: 'Sizes and full-object SHA-256' },
]
const configOptions = configurations.filter(({ id }) => [
  'versioning', 'objectLock', 'publicAccessBlock', 'lifecycle', 'cors', 'policy', 'logging', 'tagging',
].includes(id))
const evidenceLabels: Record<ObjectDifference['evidence'], string> = {
  sourceOnly: 'Source only', targetOnly: 'Destination only', differentSize: 'Different size',
  differentDigest: 'Different SHA-256', inconclusive: 'Inconclusive',
}
let rowId = 0
let listRequest = 0
let reportRequest = 0
const sourceId = ref(props.location.connection)
const targetId = ref(props.location.connection)
const sourceBuckets = ref<Bucket[]>([])
const targetBuckets = ref<Bucket[]>([])
const mappings = ref<Mapping[]>([newMapping(props.location.bucket)])
const content = ref<ContentMode>('etag')
const modifiedBefore = ref('')
const configuration = ref<Configuration[]>([])
const result = ref<BucketComparison>()
const review = ref<Review>()
const busy = ref(false)
const loading = ref(false)
const error = ref('')
const listError = ref('')
const notice = ref('')
const connections = computed(() => [
  ...(props.demo ? [{ value: 'demo', label: 'Asteria Research (demo)' }] : []),
  ...props.profiles.map(profile => ({ value: profile.id, label: profile.name })),
])
const sources = computed(() => bucketOptions(sourceBuckets.value))
const destinations = computed(() => bucketOptions(targetBuckets.value))
const sharedNames = computed(() => {
  if (sourceId.value === targetId.value) return []
  const names = new Set(targetBuckets.value.map(bucket => bucket.name))
  return sourceBuckets.value.filter(bucket => names.has(bucket.name)).map(bucket => bucket.name)
})
const valid = computed(() => {
  const targets = new Set(mappings.value.map(pair => pair.target))
  return targets.size === mappings.value.length && mappings.value.length > 0 &&
    mappings.value.every(pair => pair.source && pair.target && (pair.source !== pair.target || sourceId.value !== targetId.value)) &&
    (content.value !== 'none' || configuration.value.length > 0)
})

watch(() => props.location.connection, (id, previous) => {
  sourceId.value = id
  if (targetId.value === previous) targetId.value = id
})
watch(() => props.location.bucket, value => {
  if (value && sourceId.value === props.location.connection && mappings.value.length === 1) {
    const mapping = mappings.value[0]
    if (mapping) mapping.source = value
  }
})
watch([sourceId, targetId, () => props.sourceBuckets, () => props.profiles], () => void loadBuckets(), { immediate: true })
watch([sourceId, targetId, mappings, content, modifiedBefore, configuration], invalidate, { deep: true, flush: 'sync' })

function newMapping(source = '', target = ''): Mapping {
  return { id: ++rowId, source, target }
}

function bucketOptions(items: Bucket[]): { value: string; label: string }[] {
  return items.map(({ name }) => ({ value: name, label: name }))
}

function list(id: string): Promise<Bucket[]> {
  return id === props.location.connection ? Promise.resolve(props.sourceBuckets) : bucketsApi.list(id)
}

async function loadBuckets(): Promise<void> {
  const request = ++listRequest
  loading.value = true
  listError.value = ''
  try {
    const source = list(sourceId.value)
    const [left, right] = await Promise.all([source, sourceId.value === targetId.value ? source : list(targetId.value)])
    if (request !== listRequest) return
    sourceBuckets.value = left
    targetBuckets.value = right
    for (const mapping of mappings.value) {
      if (!left.some(bucket => bucket.name === mapping.source)) {
        const current = sourceId.value === props.location.connection ? props.location.bucket : ''
        mapping.source = mappings.value.length === 1 && left.some(bucket => bucket.name === current) ? current : ''
      }
      if (!right.some(bucket => bucket.name === mapping.target)) mapping.target = ''
    }
  } catch (failure) {
    if (request !== listRequest) return
    listError.value = errorMessage(failure)
    sourceBuckets.value = []
    targetBuckets.value = []
  } finally {
    if (request === listRequest) loading.value = false
  }
}

function invalidate(): void {
  ++reportRequest
  result.value = undefined
  review.value = undefined
  error.value = ''
  notice.value = ''
  busy.value = false
}

function autoMatch(): void {
  mappings.value = sharedNames.value.map(name => newMapping(name, name))
}

async function compare(): Promise<void> {
  if (!valid.value || busy.value || loading.value || props.demo) return
  const id = ++reportRequest
  busy.value = true
  error.value = ''
  result.value = undefined
  try {
    const report = await transfersApi.compareBuckets(sourceId.value, targetId.value, {
      pairs: mappings.value.map(({ source, target }) => ({ source, target })),
      content: content.value,
      configuration: [...configuration.value],
      modifiedBefore: content.value !== 'none' && modifiedBefore.value ? new Date(modifiedBefore.value).toISOString() : null,
    })
    if (id === reportRequest) result.value = report
  } catch (failure) {
    if (id === reportRequest) error.value = errorMessage(failure)
  } finally {
    if (id === reportRequest) busy.value = false
  }
}

function configLabel(configuration: Configuration): string {
  return configOptions.find(option => option.id === configuration)?.label ?? configuration
}

function configText(snapshot: ConfigurationSnapshot): string {
  return snapshot.status === 'unset' ? 'Unconfigured' : snapshot.value
}

function reviewConfig(pair: BucketPair, difference: ConfigurationDifference): void {
  const { configuration, source, target } = difference
  const title = source.status === 'unset' ? 'Remove destination configuration' : 'Apply configuration'
  review.value = {
    pair, action: { kind: 'configuration', configuration, source, target }, title,
    detail: `${configLabel(configuration)} for ${connectionName(targetId.value)} / ${pair.target}`,
  }
}

function canReviewObject(row: ObjectDifference): boolean {
  return !!(row.source || row.target) && [row.source, row.target].every(side => !side || !!side.etag && side.size !== null)
}

function reviewObject(pair: BucketPair, row: ObjectDifference): void {
  const { key, source, target } = row
  const title = source ? target ? 'Replace destination object' : 'Copy object to destination' : 'Delete destination object'
  review.value = {
    pair, action: { kind: 'object', key, source, target }, title,
    detail: `${JSON.stringify(key)} in ${connectionName(targetId.value)} / ${pair.target}`,
  }
}

function connectionName(id: string): string {
  return connections.value.find(option => option.value === id)?.label ?? id
}

async function apply(): Promise<void> {
  const selected = review.value
  const scope = result.value?.scope
  if (!selected || !scope || busy.value) return
  review.value = undefined
  const id = ++reportRequest
  busy.value = true
  error.value = ''
  notice.value = ''
  try {
    await transfersApi.remediate(scope, selected.pair, selected.action)
    if (id === reportRequest) {
      notice.value = `Verified destination change for ${selected.detail}.`
      busy.value = false
      await compare()
    }
  } catch (failure) {
    if (id === reportRequest) error.value = errorMessage(failure)
  } finally {
    if (id === reportRequest) busy.value = false
  }
}

function browseAvailable(connection: string): boolean {
  return workspaceAvailable(props.profiles.find(profile => profile.id === connection), 'browser')
}

function browse(connection: string, bucket: string, key: string): void {
  emit('browse', { connection, bucket, prefix: key.slice(0, key.lastIndexOf('/') + 1) })
}

async function download(connection: string, bucket: string, key: string): Promise<void> {
  error.value = ''
  try {
    const name = key.slice(key.lastIndexOf('/') + 1) || 'object'
    const path = await transfersApi.saveFile(name)
    if (!path) return
    emit('job', await jobs.start({ kind: 'download', connection, objects: [{ bucket, key, versionId: null }], target: path, directory: false }))
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}
</script>

<template>
  <div class="bucket-comparison">
    <form class="panel-form" @submit.prevent="compare">
      <div class="stage-heading"><div><h2>Bucket comparison</h2><p>Map buckets and review differences before applying a change.</p></div></div>
      <div class="form-grid">
        <label>Source connection<AppSelect v-model="sourceId" :options="connections" :disabled="busy" /></label>
        <label>Destination connection<AppSelect v-model="targetId" :options="connections" :disabled="busy" /></label>
      </div>
      <fieldset class="comparison-fieldset">
        <legend>Bucket mappings</legend>
        <div v-for="(pair, index) in mappings" :key="pair.id" class="mapping-row">
          <AppSelect v-model="pair.source" :aria-label="`Source bucket ${index + 1}`" :options="sources" :disabled="busy || loading" placeholder="Source bucket" />
          <ArrowRight :size="15" aria-hidden="true" />
          <AppSelect v-model="pair.target" :aria-label="`Destination bucket ${index + 1}`" :options="destinations" :disabled="busy || loading" placeholder="Destination bucket" />
          <button type="button" class="subtle" :aria-label="`Remove mapping ${index + 1}`" :disabled="busy || mappings.length === 1" @click="mappings.splice(index, 1)"><X :size="16" /></button>
        </div>
        <div class="comparison-actions">
          <button type="button" :disabled="busy || loading" @click="mappings.push(newMapping())"><Plus :size="14" /> Add mapping</button>
          <button type="button" :disabled="busy || loading || sharedNames.length === 0" @click="autoMatch">Match by bucket name</button>
        </div>
      </fieldset>
      <div class="form-grid">
        <label>Object comparison<AppSelect v-model="content" :options="modeOptions" :disabled="busy" /></label>
        <label>Ignore objects modified after <small>Optional</small><input v-model="modifiedBefore" type="datetime-local" :disabled="busy || content === 'none'" /></label>
      </div>
      <fieldset class="comparison-fieldset">
        <legend>Compare configuration</legend>
        <div class="comparison-options">
          <label v-for="feature in configOptions" :key="feature.id" class="check-label">
            <input v-model="configuration" :value="feature.id" type="checkbox" :disabled="busy" />{{ feature.label }}
          </label>
        </div>
      </fieldset>
      <div class="panel-actions"><button class="primary" :disabled="demo || !valid || busy || loading || !!listError">{{ busy ? 'Working…' : 'Compare buckets' }}</button></div>
    </form>

    <Notice v-if="error || listError" tone="error">{{ error || listError }}</Notice>
    <Notice v-if="notice">{{ notice }}</Notice>
    <div v-if="result" class="tool-results" aria-label="Comparison results">
      <details v-for="report in result.reports" :key="report.pair.target" class="comparison-result" open>
        <summary><strong>{{ report.pair.source }}</strong><ArrowRight :size="14" /><strong>{{ report.pair.target }}</strong></summary>
        <Notice v-if="report.contentError" tone="error">{{ report.contentError }}</Notice>
        <template v-if="report.contents">
          <dl class="result-totals">
            <div><dt>Source objects</dt><dd>{{ report.contents.sourceCount }}</dd></div>
            <div><dt>Destination objects</dt><dd>{{ report.contents.targetCount }}</dd></div>
            <div><dt>Source only</dt><dd>{{ report.contents.sourceOnly }}</dd></div>
            <div><dt>Destination only</dt><dd>{{ report.contents.targetOnly }}</dd></div>
            <div><dt>Different size</dt><dd>{{ report.contents.differentSize }}</dd></div>
            <div v-if="content === 'checksum'"><dt>Different SHA-256</dt><dd>{{ report.contents.differentDigest }}</dd></div>
            <div v-if="content === 'checksum'"><dt>Matching SHA-256</dt><dd>{{ report.contents.matchingDigest }}</dd></div>
            <div v-if="content === 'size'"><dt>Matching size</dt><dd>{{ report.contents.matchingSize }}</dd></div>
            <div><dt>Inconclusive</dt><dd>{{ report.contents.inconclusive }}</dd></div>
          </dl>
          <p v-if="report.contents.afterCutoff || report.contents.unknownModified" class="muted">
            {{ report.contents.afterCutoff }} keys after cutoff and {{ report.contents.unknownModified }} with unknown modification time were excluded.
          </p>
          <p v-if="report.contents.inconclusive" class="muted">Multipart and encrypted ETags are inconclusive. A matching ETag alone does not verify content.</p>
          <details v-if="report.contents.details.length">
            <summary>Object evidence ({{ report.contents.details.length }} shown<span v-if="report.contents.hidden">, {{ report.contents.hidden }} omitted</span>)</summary>
            <div v-for="row in report.contents.details" :key="row.key" class="comparison-entry">
              <div><code class="object-key">{{ JSON.stringify(row.key) }}</code><p class="muted">{{ evidenceLabels[row.evidence] }}<template v-if="row.evidence === 'inconclusive' && row.matchingEtag !== null"> · {{ row.matchingEtag ? 'Matching' : 'Different' }} ETags</template></p>
                <p v-if="row.issue" class="muted">{{ row.issue }}</p>
              </div>
              <div class="comparison-sides">
                <div v-if="row.source"><strong>Source</strong> {{ formatSize(row.source.size) }}
                  <button v-if="browseAvailable(result.scope.sourceConnection)" type="button" class="subtle" @click="browse(result.scope.sourceConnection, report.pair.source, row.key)">Browse source</button>
                  <button type="button" class="subtle" @click="download(result.scope.sourceConnection, report.pair.source, row.key)">Download source</button>
                </div>
                <div v-if="row.target"><strong>Destination</strong> {{ formatSize(row.target.size) }}
                  <button v-if="browseAvailable(result.scope.targetConnection)" type="button" class="subtle" @click="browse(result.scope.targetConnection, report.pair.target, row.key)">Browse destination</button>
                  <button type="button" class="subtle" @click="download(result.scope.targetConnection, report.pair.target, row.key)">Download destination</button>
                </div>
              </div>
              <button v-if="canReviewObject(row)" type="button" :disabled="busy" @click="reviewObject(report.pair, row)">Review object change</button>
            </div>
          </details>
        </template>
        <details v-for="row in report.configuration" :key="row.configuration" class="comparison-configuration">
          <summary>{{ configLabel(row.configuration) }} · {{ row.source.status === 'error' || row.target.status === 'error' ? 'Read error' : row.different ? 'Different' : 'Matching' }}</summary>
          <div class="comparison-documents">
            <div><strong>Source</strong><pre>{{ configText(row.source) }}</pre></div>
            <div><strong>Destination</strong><pre>{{ configText(row.target) }}</pre></div>
          </div>
          <button v-if="row.canApply" type="button" :disabled="busy" @click="reviewConfig(report.pair, row)">Review configuration change</button>
        </details>
      </details>
    </div>
    <ConfirmDialog v-if="review" :title="review.title" :description="review.detail" :action="review.title"
      :danger="review.action.source === null || review.action.kind === 'configuration' && review.action.source.status === 'unset'"
      @close="review = undefined" @confirm="apply" />
  </div>
</template>

<style scoped>
.bucket-comparison, .comparison-fieldset, .comparison-entry { display: grid; gap: 9px; min-width: 0; }
.comparison-fieldset { margin: 0; padding: 0; border: 0; }
.comparison-fieldset legend { margin-bottom: 6px; padding: 0; font-weight: 650; }
.mapping-row { display: grid; align-items: center; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto; gap: 8px; }
.comparison-actions, .comparison-options { display: flex; flex-wrap: wrap; gap: 8px 14px; }
.comparison-result, .comparison-configuration, .comparison-entry { padding: 8px 0; border-bottom: 1px solid var(--line); }
.comparison-result summary { cursor: pointer; padding-block: 6px; }
.comparison-result summary svg { margin-inline: 8px; vertical-align: middle; }
.comparison-entry > button { justify-self: start; }
.comparison-entry p { margin: 3px 0; }
.comparison-sides, .comparison-documents { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.comparison-documents pre { max-height: 16rem; overflow: auto; padding: 8px; background: var(--surface-alt); }
.object-key { white-space: pre-wrap; overflow-wrap: anywhere; }
@media (max-width: 680px) {
  .comparison-sides, .comparison-documents { grid-template-columns: minmax(0, 1fr); }
}
</style>
