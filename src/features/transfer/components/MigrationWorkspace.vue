<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { FolderSync, Plus } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import { formatDate } from '../../../common/format'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { Bucket } from '../../bucket/model/bucket'
import type { Connection } from '../../connection/model/connection'
import { migrationsApi } from '../api/migrations'
import { bucketLabels, migrationStatus, type Migration, type MigrationAction, type MigrationDraft } from '../model/migration'

const props = defineProps<{ connection: string; profiles: Connection[]; buckets: Bucket[]; demo: boolean }>()
const records = ref<Migration[]>([])
const selected = ref<Migration>()
const draft = ref<MigrationDraft>()
const error = ref('')
const busy = ref(false)
const confirming = ref(false)
interface ExecutionConfirmation {
  action: MigrationAction
  title: string
  description: string
  label: string
  confirmation: string
  typed: boolean
  danger: boolean
}
const executionConfirmation = ref<ExecutionConfirmation>()
const execution = computed(() => selected.value?.run)
const running = computed(() => execution.value?.status === 'copying' || execution.value?.status === 'cuttingOver')
const cleanupAvailable = computed(() => execution.value && !running.value && execution.value.status !== 'awaitingCutover'
  && !selected.value?.recoveryRequired)
const incompleteNames = computed(() => cleanupAvailable.value ? execution.value?.buckets
  .filter(bucket => bucket.owned && bucket.status !== 'removed' && bucket.status !== 'verified').map(bucket => bucket.mapping.target) ?? [] : [])
const sourceNames = computed(() => cleanupAvailable.value ? execution.value?.buckets
  .filter(bucket => bucket.owned && bucket.sourceProtected && bucket.status === 'verified' && !bucket.sourceDeleted)
  .map(bucket => bucket.mapping.source) ?? [] : [])
const now = ref(Date.now())
const connectionOptions = computed(() => props.profiles
  .filter(profile => profile.settings?.managerEnabled !== false)
  .map(profile => ({ value: profile.id, label: profile.name })))
const modeOptions = [{ value: 'preCopy', label: 'Pre-copy, then manual cutover' }, { value: 'immediate', label: 'Immediate cutover' }]
const methodOptions = [{ value: 'streamed', label: 'Stream through StoreForge' }, { value: 'storage', label: 'Copy within storage' }]
const report = computed(() => selected.value?.report)
const status = computed(() => selected.value ? migrationStatus(selected.value, now.value) : 'Draft')
const availableBuckets = computed(() => [...new Set([
  ...props.buckets.map(bucket => bucket.name), ...(draft.value?.mappings.map(mapping => mapping.source) ?? []),
])])
let timer: ReturnType<typeof setInterval> | undefined
let request = 0

onMounted(() => { timer = setInterval(tick, 1000) })
onUnmounted(() => { clearInterval(timer); request++ })
watch(() => [props.connection, props.demo], reset, { immediate: true })

function tick(): void {
  now.value = Date.now()
  if (records.value.some(record => record.checking || record.run?.status === 'copying' || record.run?.status === 'cuttingOver')
    && !busy.value) void refresh()
}

function reset(): void {
  request++
  records.value = []
  selected.value = undefined
  draft.value = undefined
  error.value = ''
  busy.value = false
  confirming.value = false
  executionConfirmation.value = undefined
  if (!props.demo) void refresh()
}

async function refresh(): Promise<void> {
  const current = request
  try {
    const listing = await migrationsApi.list(props.connection)
    if (current !== request) return
    records.value = listing
    if (selected.value) selected.value = listing.find(record => record.id === selected.value?.id)
  } catch (failure) {
    if (current === request) error.value = errorMessage(failure)
  }
}

function createDraft(): void {
  error.value = ''
  draft.value = {
    source: props.connection, target: props.connection, mappings: [], mode: 'preCopy', method: 'streamed',
    history: false, settings: false, parallelism: 4, protectDestination: false, grantSourceRead: false,
  }
}

function open(record: Migration): void {
  selected.value = record
  error.value = ''
}

function back(): void {
  if (!selected.value || !draft.value) selected.value = undefined
  draft.value = undefined
  error.value = ''
}

function edit(): void {
  const existing = selected.value?.draft
  if (existing) draft.value = { ...existing, mappings: existing.mappings.map(mapping => ({ ...mapping })) }
}

function toggleMapping(source: string, checked: boolean): void {
  if (!draft.value) return
  if (checked) draft.value.mappings.push({ source, target: '' })
  else draft.value.mappings = draft.value.mappings.filter(mapping => mapping.source !== source)
}

function setMode(mode: string): void {
  if (draft.value && (mode === 'preCopy' || mode === 'immediate')) draft.value.mode = mode
}

function setMethod(method: string): void {
  if (!draft.value || (method !== 'streamed' && method !== 'storage')) return
  draft.value.method = method
  if (method === 'streamed') draft.value.grantSourceRead = false
}

async function run(action: () => Promise<Migration | null>): Promise<void> {
  if (busy.value) return
  const current = ++request
  error.value = ''
  busy.value = true
  try {
    const record = await action()
    if (current !== request) return
    if (record) {
      selected.value = record
      records.value = [record, ...records.value.filter(existing => existing.id !== record.id)]
    } else {
      records.value = records.value.filter(existing => existing.id !== selected.value?.id)
      selected.value = undefined
    }
  } catch (failure) {
    if (current === request) error.value = errorMessage(failure)
  } finally {
    if (current === request) busy.value = false
  }
}

async function save(check: boolean): Promise<void> {
  const input = draft.value
  if (!input) return
  await run(async () => {
    const record = await migrationsApi.save(input, selected.value)
    if (props.connection !== input.source) return record
    selected.value = record
    records.value = [record, ...records.value.filter(existing => existing.id !== record.id)]
    draft.value = undefined
    return check ? migrationsApi.check(record, 'read') : record
  })
}

function check(approved: boolean): void {
  confirming.value = false
  const record = selected.value
  if (record) void run(() => migrationsApi.check(record, approved ? 'active' : 'read', approved))
}

function recover(): void {
  const record = selected.value
  if (record) void run(() => migrationsApi.restore(record))
}

function confirmExecution(action: MigrationAction): void {
  const record = selected.value
  if (!record) return
  if (action === 'start' && record.draft.mode === 'preCopy') {
    executionConfirmation.value = {
      action, title: 'Start pre-copy?', description: 'StoreForge will create the reviewed destination buckets. Source writes remain available until cutover.',
      label: 'Start copy', confirmation: 'START COPY', typed: false, danger: false,
    }
  } else if (action === 'start' || action === 'cutover') {
    executionConfirmation.value = {
      action, title: 'Start cutover?', description: 'StoreForge will block source writes for the selected buckets, copy changes, and verify content, tags, and history.',
      label: 'Start cutover', confirmation: 'CUTOVER', typed: true, danger: false,
    }
  } else if (action === 'stop') {
    executionConfirmation.value = {
      action, title: 'Stop migration?', description: 'StoreForge will finish in-flight requests, restore recorded access, and retain the bucket data.',
      label: 'Stop migration', confirmation: '', typed: false, danger: false,
    }
  } else if (action === 'removeIncomplete' || action === 'deleteSources') {
    const fromSource = action === 'deleteSources'
    executionConfirmation.value = {
      action, title: fromSource ? 'Delete verified source buckets?' : 'Delete incomplete destinations?',
      description: fromSource
        ? 'StoreForge will compare fresh SHA-256 content, exact tags, and ordered versions before deleting the source buckets and all their versions.'
        : 'StoreForge will verify ownership before deleting these incomplete destinations and all their contents.',
      label: fromSource ? 'Delete source buckets' : 'Delete incomplete destinations',
      confirmation: `DELETE ${(fromSource ? sourceNames.value : incompleteNames.value).join(', ')}`,
      typed: true, danger: true,
    }
  } else if (action === 'deleteRecord') {
    executionConfirmation.value = {
      action, title: 'Delete migration record?', description: 'This removes the saved migration history. Buckets and their contents are retained.',
      label: 'Delete record', confirmation: 'DELETE RECORD', typed: true, danger: true,
    }
  }
}

function perform(action: MigrationAction, confirmation = ''): void {
  const record = selected.value
  executionConfirmation.value = undefined
  if (record) void run(() => migrationsApi.action(record, action, confirmation || null))
}

function performConfirmed(): void {
  const choice = executionConfirmation.value
  if (choice) perform(choice.action, choice.confirmation)
}

function profileName(id: string): string {
  return props.profiles.find(profile => profile.id === id)?.name ?? 'Unavailable connection'
}
</script>

<template>
  <div class="migration-workspace panel-form">
    <div class="stage-heading">
      <div>
        <h2>{{ draft ? (selected ? 'Edit migration' : 'New migration') : selected ? 'Migration details' : 'Bucket migrations' }}</h2>
        <p v-if="!selected && !draft">Map source buckets to new destinations and review provider checks before copying.</p>
        <p v-else-if="selected && !draft">{{ profileName(selected.draft.source) }} → {{ profileName(selected.draft.target) }} · {{ status }}</p>
      </div>
      <button v-if="selected || draft" :disabled="busy" @click="back">{{ draft && selected ? 'Cancel edit' : 'All migrations' }}</button>
      <button v-else class="primary" :disabled="demo || busy" @click="createDraft"><Plus :size="15" />New migration</button>
    </div>
    <Notice v-if="error" tone="error" role="alert">{{ error }}</Notice>
    <Notice v-if="busy" role="status">Working with the selected connections…</Notice>
    <template v-if="!selected && !draft">
      <EmptyState v-if="!records.length" :icon="FolderSync" title="No saved migrations" description="Saved drafts and check results appear here." />
      <table v-else class="report-table">
        <thead><tr><th>Source buckets</th><th>Destination connection</th><th>Status</th><th>Updated</th><th>Details</th></tr></thead>
        <tbody>
          <tr v-for="record in records" :key="record.id">
            <td>{{ record.draft.mappings.map(mapping => mapping.source).join(', ') }}</td>
            <td>{{ profileName(record.draft.target) }}</td><td>{{ migrationStatus(record, now) }}</td>
            <td>{{ formatDate(record.updatedAt, true) }}</td><td><button @click="open(record)">Open migration</button></td>
          </tr>
        </tbody>
      </table>
    </template>
    <form v-if="draft" class="migration-form" @submit.prevent="save(true)">
      <fieldset :disabled="busy" class="migration-fields">
        <div class="form-grid">
          <label>Destination connection<AppSelect v-model="draft.target" :options="connectionOptions" /></label>
          <label>Migration mode<AppSelect :model-value="draft.mode" :options="modeOptions" @update:model-value="setMode" /></label>
          <label>Copy method<AppSelect :model-value="draft.method" :options="methodOptions" @update:model-value="setMethod" /></label>
          <label>Parallel buckets<input v-model.number="draft.parallelism" type="number" min="1" max="32" required /></label>
        </div>
        <fieldset class="migration-fields">
          <legend>Bucket mappings</legend>
          <p v-if="!availableBuckets.length" class="muted">No source buckets are available on this connection.</p>
          <label v-for="bucket in availableBuckets" :key="bucket" class="check-label">
            <input type="checkbox" :checked="draft.mappings.some(mapping => mapping.source === bucket)"
              @change="toggleMapping(bucket, ($event.target as HTMLInputElement).checked)" />{{ bucket }}
          </label>
          <div v-for="mapping in draft.mappings" :key="mapping.source" class="form-grid">
            <label class="span-two">Destination for {{ mapping.source }}<input v-model="mapping.target"
              autocomplete="off" spellcheck="false" required placeholder="New bucket name" /></label>
          </div>
        </fieldset>
        <label class="check-label"><input v-model="draft.history" type="checkbox" /> Copy versions and delete markers</label>
        <label class="check-label"><input v-model="draft.settings" type="checkbox" /> Copy supported bucket settings</label>
        <label class="check-label"><input v-model="draft.protectDestination" type="checkbox" /> Restrict destination writes to the migration principal</label>
        <label v-if="draft.method === 'storage'" class="check-label"><input v-model="draft.grantSourceRead" type="checkbox" /> Temporarily grant the destination principal read access to the source</label>
      </fieldset>
      <div class="panel-actions">
        <button type="button" :disabled="busy || !draft.mappings.length" @click="save(false)">Save draft</button>
        <button class="primary" :disabled="busy || !draft.mappings.length">Save and run read-only checks</button>
      </div>
    </form>
    <template v-else-if="selected">
      <Notice v-if="selected.recoveryRequired" tone="error" role="alert">{{ selected.recoveryError || 'Restore access before editing or running checks.' }}</Notice>
      <dl class="result-totals">
        <div><dt>Mode</dt><dd>{{ modeOptions.find(option => option.value === selected?.draft.mode)?.label }}</dd></div>
        <div><dt>Copy method</dt><dd>{{ methodOptions.find(option => option.value === selected?.draft.method)?.label }}</dd></div>
        <div><dt>Content</dt><dd>{{ selected.draft.history ? 'Versions and delete markers' : 'Current objects' }}</dd></div>
        <div><dt>Parallel buckets</dt><dd>{{ selected.draft.parallelism }}</dd></div>
      </dl>
      <table class="report-table">
        <thead><tr><th>Source bucket</th><th>New destination</th><th>Preparation</th></tr></thead>
        <tbody>
          <tr v-for="mapping in selected.draft.mappings" :key="mapping.source">
            <td><code>{{ mapping.source }}</code></td><td><code>{{ mapping.target }}</code></td>
            <td v-if="report?.pairs.some(pair => pair.mapping.source === mapping.source)">
              {{ report.pairs.find(pair => pair.mapping.source === mapping.source)?.issues.length ? 'Blocked' : 'Passed' }}
            </td>
            <td v-else>Pending</td>
          </tr>
        </tbody>
      </table>
      <template v-if="report">
        <p class="muted">Checked {{ formatDate(report.checkedAt, true) }} · Expires {{ formatDate(report.expiresAt, true) }}</p>
        <details v-for="pair in report.pairs" :key="pair.mapping.source" class="migration-pair" :open="pair.issues.length > 0">
          <summary>{{ pair.mapping.source }} → {{ pair.mapping.target }}</summary>
          <ul v-if="pair.issues.length"><li v-for="issue in pair.issues" :key="issue">{{ issue }}</li></ul>
          <p v-if="pair.sample">Representative object <code>{{ pair.sample }}</code></p>
          <p v-else>Object reads and tags could not be sampled. The source will be checked again before copying.</p>
          <p>Settings to copy · {{ pair.settingsCopied.join(', ') || 'None' }}</p>
          <p>Settings outside this plan · {{ pair.settingsOmitted.join(', ') }}</p>
        </details>
      </template>
      <section v-if="execution" class="migration-form" aria-label="Copy progress">
        <h3>Copy progress</h3>
        <table class="report-table">
          <thead><tr><th>Bucket mapping</th><th>Status</th><th>Keys</th><th>Details</th></tr></thead>
          <tbody>
            <tr v-for="bucket in execution.buckets" :key="bucket.mapping.source">
              <td><code>{{ bucket.mapping.source }}</code> → <code>{{ bucket.mapping.target }}</code></td>
              <td>{{ bucketLabels[bucket.status] }}</td><td>{{ bucket.completed }} / {{ bucket.total }}</td>
              <td>
                <p v-if="bucket.currentKey">Current key <code>{{ bucket.currentKey }}</code></p>
                <p v-if="bucket.error">{{ bucket.error }}</p>
                <p v-if="bucket.cleanupError">{{ bucket.cleanupError }}</p>
                <p v-if="bucket.sourceDeleted">Source deleted</p>
                <p v-else-if="bucket.sourceProtected">Source access changes recorded</p>
              </td>
            </tr>
          </tbody>
        </table>
      </section>
      <div class="panel-actions">
        <button v-if="selected.canRestore && !running" class="primary" :disabled="busy" @click="recover">Restore access</button>
        <template v-if="execution">
          <template v-if="running">
            <button :disabled="busy" @click="perform('pause')">Pause copy</button>
            <button :disabled="busy" @click="confirmExecution('stop')">Stop migration</button>
          </template>
          <template v-else-if="!selected.recoveryRequired">
            <button v-if="execution.status === 'paused'" class="primary" :disabled="busy" @click="perform('resume')">Resume copy</button>
            <button v-if="execution.status === 'failed'" :disabled="busy" @click="perform('retry')">Retry failed buckets</button>
            <button v-if="execution.status === 'awaitingCutover'" class="primary" :disabled="busy" @click="confirmExecution('cutover')">Review cutover</button>
            <button v-if="['paused', 'awaitingCutover', 'failed'].includes(execution.status)" :disabled="busy" @click="confirmExecution('stop')">Stop migration</button>
            <button v-if="incompleteNames.length" class="danger-outline" :disabled="busy" @click="confirmExecution('removeIncomplete')">Delete incomplete destinations</button>
            <button v-if="sourceNames.length" class="danger-outline" :disabled="busy" @click="confirmExecution('deleteSources')">Delete source buckets</button>
          </template>
        </template>
        <template v-else-if="!selected.recoveryRequired">
          <button :disabled="busy || !!selected.checking" @click="edit">Edit draft</button>
          <button :disabled="busy || !!selected.checking" @click="check(false)">Run read-only checks</button>
          <button v-if="status === 'Ready for active checks'" class="primary" :disabled="busy" @click="confirming = true">Review active checks</button>
          <button v-if="status === 'Reviewed'" class="primary" :disabled="busy" @click="confirmExecution('start')">Review start copy</button>
        </template>
        <button v-if="!selected.canRestore && !running && !selected.checking" :disabled="busy" @click="confirmExecution('deleteRecord')">Delete record</button>
      </div>
    </template>
    <ConfirmDialog v-if="executionConfirmation" :title="executionConfirmation.title" :description="executionConfirmation.description"
      :action="executionConfirmation.label" :phrase="executionConfirmation.typed ? executionConfirmation.confirmation : undefined"
      :danger="executionConfirmation.danger" @close="executionConfirmation = undefined" @confirm="performConfirmed" />
    <ConfirmDialog v-if="confirming" title="Run active checks?"
      description="StoreForge temporarily blocks source writes, creates a disposable destination, and tests object, tag, multipart, and configuration access. It restores the recorded source policy when the checks finish."
      action="Run active checks" @close="confirming = false" @confirm="check(true)">
      <p v-if="selected?.draft.grantSourceRead">The destination principal will also receive a temporary source read grant.</p>
    </ConfirmDialog>
  </div>
</template>

<style scoped>
.migration-workspace, .migration-form, .migration-fields { display: grid; gap: 12px; min-width: 0; }
.migration-fields { margin: 0; padding: 12px; border: 1px solid var(--line); border-radius: 8px; }
.migration-fields legend { padding: 0 5px; }
.migration-pair { padding: 9px 12px; border: 1px solid var(--line); border-radius: 8px; }
.migration-pair summary { cursor: pointer; }
.migration-pair p { overflow-wrap: anywhere; }
</style>
