<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ArrowLeftRight, ChartNoAxesColumn, Database, FolderSync, ListChecks, Trash2 } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import { bucketsApi } from '../../bucket/api/buckets'
import type { Bucket } from '../../bucket/model/bucket'
import { objectsApi } from '../../browser/api/objects'
import type { Location, MultipartUpload } from '../../browser/model/objects'
import type { Connection } from '../../connection/model/connection'
import { transfersApi } from '../api/transfers'
import type { Comparison, Integrity, Transfer, Usage } from '../model/transfer'
import { useJobs } from '../store/jobs'

const props = defineProps<{ location: Location; profiles: Connection[]; demo: boolean }>()
const emit = defineEmits<{ job: [id: string] }>()
const jobs = useJobs()
const options = [
  { name: 'Compare', icon: ArrowLeftRight },
  { name: 'Migrate', icon: FolderSync },
  { name: 'Usage', icon: ChartNoAxesColumn },
  { name: 'Integrity', icon: ListChecks },
  { name: 'Multipart', icon: Database },
  { name: 'Purge', icon: Trash2 },
] as const
const tab = ref<(typeof options)[number]['name']>('Compare')
const targetId = ref(props.location.connection)
const targetBucket = ref('')
const targetPrefix = ref('')
const targetBuckets = ref<Bucket[]>([])
const targetError = ref('')
const includeVersions = ref(false)
const removeBucket = ref(false)
const overwrite = ref(false)
const confirm = ref<'Migrate' | 'Purge'>()
const aborting = ref<MultipartUpload>()
const busy = ref(false)
const error = ref('')
const comparison = ref<Comparison>()
const usage = ref<Usage>()
const integrity = ref<Integrity>()
const multipart = ref<MultipartUpload[]>()
const needsTarget = computed(() => tab.value === 'Compare' || tab.value === 'Migrate')
const targetConnections = computed(() => [
  ...(props.demo ? [{ value: 'demo', label: 'Asteria Research (demo)' }] : []),
  ...props.profiles.map(profile => ({ value: profile.id, label: profile.name })),
])
const targetBucketOptions = computed(() => targetBuckets.value.map(({ name }) => ({ value: name, label: name })))
const target = computed<Location>(() => ({ connection: targetId.value, bucket: targetBucket.value, prefix: targetPrefix.value }))
const sameLocation = computed(() => props.location.connection === target.value.connection && props.location.bucket === target.value.bucket && props.location.prefix === target.value.prefix)
const overlap = computed(() => props.location.connection === target.value.connection && props.location.bucket === target.value.bucket &&
  (props.location.prefix.startsWith(target.value.prefix) || target.value.prefix.startsWith(props.location.prefix)))
const differenceNames = { sourceOnly: 'Source only', targetOnly: 'Destination only', changed: 'Different size', differentEtag: 'Different ETag' }
let targetRequest = 0
let request = 0

watch(() => props.location.connection, id => { targetId.value = id })
watch(targetId, () => void loadTargetBuckets(), { immediate: true })
watch([tab, () => props.location, target], () => resetResults())

function resetResults(): void {
  request++
  busy.value = false
  error.value = ''
  comparison.value = undefined
  usage.value = undefined
  integrity.value = undefined
  multipart.value = undefined
}

async function loadTargetBuckets(): Promise<void> {
  const id = ++targetRequest
  targetBuckets.value = []
  targetBucket.value = ''
  targetError.value = ''
  if (!targetId.value) return
  try {
    const listing = await bucketsApi.list(targetId.value)
    if (id === targetRequest) targetBuckets.value = listing
  } catch (failure) {
    if (id === targetRequest) targetError.value = errorMessage(failure)
  }
}

function review(): void {
  if (needsTarget.value && !targetBucket.value) return
  if (tab.value === 'Migrate' || tab.value === 'Purge') confirm.value = tab.value
  else void run()
}

async function run(): Promise<void> {
  const id = ++request
  busy.value = true
  error.value = ''
  try {
    const location = props.location
    if (tab.value === 'Compare') {
      const result = await transfersApi.compare(location, target.value)
      if (id === request) comparison.value = result
    } else if (tab.value === 'Usage') {
      const result = await transfersApi.usage(location, includeVersions.value)
      if (id === request) usage.value = result
    } else if (tab.value === 'Integrity') {
      const result = await transfersApi.integrity(location)
      if (id === request) integrity.value = result
    } else if (tab.value === 'Multipart') {
      const result = await objectsApi.multipart(location)
      if (id === request) multipart.value = result
    }
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request) busy.value = false
  }
}

async function start(): Promise<void> {
  const action = confirm.value
  confirm.value = undefined
  if (!action) return
  const transfer: Transfer = action === 'Migrate'
    ? { kind: 'migrate', source: props.location, target: target.value, overwrite: overwrite.value }
    : { kind: 'purge', source: props.location, includeVersions: includeVersions.value, removeBucket: removeBucket.value }
  busy.value = true
  error.value = ''
  try {
    emit('job', await jobs.start(transfer))
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}

async function abort(): Promise<void> {
  const upload = aborting.value
  aborting.value = undefined
  if (!upload) return
  try {
    await objectsApi.abortMultipart(props.location.connection, props.location.bucket, upload.key, upload.uploadId)
    await run()
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}
</script>

<template>
  <section class="workspace-page">
    <div class="page-header">
      <div><h1>Storage tools</h1><p>Compare, migrate, and maintain objects under a bucket or prefix.</p></div>
      <span v-if="location.bucket" class="context-chip"><Database :size="16" /><code>{{ location.bucket }}/{{ location.prefix }}</code></span>
    </div>
    <EmptyState v-if="!location.bucket" :icon="Database" title="Select a source bucket" description="Choose a bucket from the sidebar. Open a folder in Objects to target a specific prefix." />
    <template v-else>
      <Notice v-if="demo">Select a live connection to scan, migrate, or change objects.</Notice>
      <nav class="workspace-tabs tool-tabs" aria-label="Storage tools">
        <button v-for="option in options" :key="option.name" :aria-pressed="tab === option.name" :class="{ active: tab === option.name }" @click="tab = option.name">
          <component :is="option.icon" :size="16" />{{ option.name }}
        </button>
      </nav>
      <div class="stage-panel tools-panel">
        <FadeRegion>
          <div :key="tab">
            <form class="panel-form" @submit.prevent="review">
              <div class="stage-heading">
                <div><h2>{{ tab }}</h2>
                  <p v-if="tab === 'Compare'">Compare relative keys, sizes, and ETags across two locations.</p>
                  <p v-else-if="tab === 'Migrate'">Copy all current objects under the source prefix to a destination.</p>
                  <p v-else-if="tab === 'Usage'">Count objects and bytes with an optional scan of versions and delete markers.</p>
                  <p v-else-if="tab === 'Integrity'">Check each listed key against the metadata currently returned by the endpoint.</p>
                  <p v-else-if="tab === 'Multipart'">Inspect and abort incomplete multipart uploads under this prefix.</p>
                  <p v-else>Delete current objects or permanently remove all versions under this prefix.</p>
                </div>
              </div>
              <div v-if="needsTarget" class="form-grid">
                <label>Destination connection<AppSelect v-model="targetId" :options="targetConnections" /></label>
                <label>Destination bucket<AppSelect v-model="targetBucket" :options="targetBucketOptions" placeholder="Choose a bucket" required /></label>
                <label class="span-two">Destination prefix <small>Optional</small><input v-model="targetPrefix" placeholder="e.g. telemetry/" spellcheck="false" /></label>
              </div>
              <label v-if="tab === 'Migrate'" class="check-label"><input v-model="overwrite" type="checkbox" /> Overwrite existing destination objects</label>
              <label v-if="tab === 'Usage' || tab === 'Purge'" class="check-label"><input v-model="includeVersions" type="checkbox" /> Include all versions and delete markers</label>
              <label v-if="tab === 'Purge' && !location.prefix && includeVersions" class="check-label"><input v-model="removeBucket" type="checkbox" /> Delete the bucket after emptying it</label>
              <Notice v-if="error || (needsTarget && targetError)" tone="error">{{ error || targetError }}</Notice>
              <Notice v-if="tab === 'Migrate' && targetBucket && overlap" tone="error">Choose a destination outside the source prefix.</Notice>
              <div class="panel-actions"><button :class="tab === 'Purge' ? 'danger' : 'primary'" :disabled="demo || busy || (needsTarget && (!targetBucket || sameLocation || (tab === 'Migrate' && overlap)))">
                {{ busy ? 'Working…' : tab === 'Purge' ? 'Review purge' : tab === 'Migrate' ? 'Review migration' : tab === 'Multipart' ? 'List uploads' : `Run ${tab.toLowerCase()} scan` }}
              </button></div>
            </form>

            <div v-if="comparison" class="tool-results">
              <dl class="result-totals">
                <div><dt>Matching size and ETag</dt><dd>{{ comparison.matching }}</dd></div>
                <div><dt>Different size</dt><dd>{{ comparison.changed }}</dd></div>
                <div><dt>Different ETag</dt><dd>{{ comparison.differentEtag }}</dd></div>
                <div><dt>Source only</dt><dd>{{ comparison.sourceOnly }}</dd></div>
                <div><dt>Destination only</dt><dd>{{ comparison.targetOnly }}</dd></div>
              </dl>
              <p class="muted">Multipart and encrypted objects can have different ETags even when their contents match.</p>
              <table v-if="comparison.differences.length" class="report-table">
                <thead><tr><th>Relative key</th><th>Difference</th><th>Source</th><th>Destination</th></tr></thead>
                <tbody><tr v-for="row in comparison.differences" :key="row.key"><td><code>{{ row.key }}</code></td><td>{{ differenceNames[row.difference] }}</td><td>{{ formatSize(row.sourceBytes) }}</td><td>{{ formatSize(row.targetBytes) }}</td></tr></tbody>
              </table>
            </div>
            <dl v-if="usage" class="result-totals tool-results">
              <div><dt>Current objects</dt><dd>{{ usage.objects }}</dd></div><div><dt>Current size</dt><dd>{{ formatSize(usage.bytes) }}</dd></div>
              <div v-if="includeVersions"><dt>Object versions</dt><dd>{{ usage.versions }}</dd></div><div v-if="includeVersions"><dt>Delete markers</dt><dd>{{ usage.deleteMarkers }}</dd></div>
            </dl>
            <div v-if="integrity" class="tool-results">
              <p>{{ integrity.checked }} objects checked · {{ integrity.failed }} failed</p>
              <ul v-if="integrity.failures.length" class="failure-list"><li v-for="failure in integrity.failures" :key="failure.item"><code>{{ failure.item }}</code><span>{{ failure.message }}</span></li></ul>
            </div>
            <div v-if="multipart" class="tool-results">
              <p v-if="!multipart.length" class="muted">No incomplete uploads.</p>
              <div v-for="upload in multipart" :key="upload.uploadId" class="settings-row">
                <div class="settings-description"><strong>{{ upload.key }}</strong><p>{{ formatDate(upload.initiated, true) }} · {{ upload.uploadId }}</p></div>
                <button class="danger-outline" @click="aborting = upload">Abort upload</button>
              </div>
            </div>
          </div>
        </FadeRegion>
      </div>
    </template>
    <ConfirmDialog v-if="confirm" :title="confirm === 'Purge' ? 'Purge this location?' : 'Start migration?'"
      :description="confirm === 'Purge' ? `Delete ${includeVersions ? 'all versions and delete markers' : 'current objects'} in ${location.bucket}/${location.prefix}.` : `Copy current objects to ${target.bucket}/${target.prefix}. ${overwrite ? 'Existing destination keys may be overwritten.' : 'Existing keys are protected.'}`"
      :action="confirm === 'Purge' ? 'Purge objects' : 'Start migration'" :phrase="confirm === 'Purge' ? location.bucket : undefined" :danger="confirm === 'Purge'" @close="confirm = undefined" @confirm="start" />
    <ConfirmDialog v-if="aborting" title="Abort multipart upload?" :description="`Incomplete parts for ${aborting.key} will be discarded.`" action="Abort upload" danger @close="aborting = undefined" @confirm="abort" />
  </section>
</template>
