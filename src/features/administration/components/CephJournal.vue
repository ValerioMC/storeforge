<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { Download } from 'lucide-vue-next'
import { formatDate } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { exportCephJournal, readMultisiteJournal } from '../api/ceph-multisite'
import {
  appendJournalPage, availabilityLabels, filterJournal, journalCsv, journalNextGeneration, journalNextPage, journalRequest,
  type BucketIdentity, type JournalPage, type JournalScope, type JournalSelection, type LoadedJournal, type Observation, type ZoneBinding,
} from '../model/ceph-multisite'

const props = defineProps<{
  period: string
  endpoints: { label: string; value: string; binding: ZoneBinding }[]
  preset?: { gateway: string; bucket: BucketIdentity; generation: string }
}>()
const families = [{ value: 'metadata', label: 'Metadata (mdlog)' }, { value: 'data', label: 'Data (datalog)' },
  { value: 'bilog', label: 'Bucket index (bilog)' }] as const
const input = reactive<JournalSelection>({
  family: 'metadata', gateway: '', period: props.period, shard: '0',
  bucket: { name: '', tenant: '', instance: null }, generation: '',
})
const loaded = ref<LoadedJournal>()
const unavailable = ref<Observation<JournalPage>>()
const busy = ref(false)
const exporting = ref(false)
const error = ref('')
const filter = ref('')
const scope = computed(() => {
  const zone = props.endpoints.find(endpoint => endpoint.value === input.gateway)?.binding
  return zone ? journalRequest(input, zone) : undefined
})
const filtered = computed(() => filterJournal(loaded.value?.entries ?? [], filter.value))
const nextPage = computed(() => loaded.value ? journalNextPage(loaded.value) : undefined)
const nextGeneration = computed(() => loaded.value ? journalNextGeneration(loaded.value) : undefined)
const loadedScopeLabel = computed(() => {
  const view = loaded.value
  if (!view) return ''
  const gateway = props.endpoints.find(endpoint => endpoint.value === view.scope.target.scope.connection)?.label
  const { journal } = view.scope
  if (journal.kind === 'metadata') return `${gateway} · Period ${journal.period} · Shard ${journal.shard}`
  if (journal.kind === 'data') return `${gateway} · Shard ${journal.shard}`
  return `${gateway} · ${journal.bucket.tenant || 'Default tenant'} / ${journal.bucket.name} · ${journal.bucket.instance} · Generation ${journal.generation}${journal.shard === null ? '' : ` · Shard ${journal.shard}`}`
})

watch(() => props.preset, preset => {
  if (!preset || busy.value) return
  input.family = 'bilog'
  input.gateway = preset.gateway
  input.bucket = { ...preset.bucket }
  input.shard = ''
  input.generation = preset.generation
  loaded.value = undefined
  unavailable.value = undefined
  error.value = ''
}, { immediate: true })

function resetBucketBinding(): void {
  input.bucket.instance = null
}

async function load(request: JournalScope, append = false): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  unavailable.value = undefined
  if (!append) {
    loaded.value = undefined
    filter.value = ''
  }
  try {
    const response = await readMultisiteJournal(request)
    if (response.data) loaded.value = appendJournalPage(request, response.data, append ? loaded.value : undefined)
    else unavailable.value = response
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}

function loadGeneration(): void {
  const next = nextGeneration.value
  if (!next || next.journal.kind !== 'bilog') return
  input.family = 'bilog'
  input.gateway = next.target.scope.connection
  input.bucket = { ...next.journal.bucket }
  input.generation = next.journal.generation
  input.shard = next.journal.shard?.toString() ?? ''
  void load(next)
}

async function exportRows(): Promise<void> {
  exporting.value = true
  error.value = ''
  try {
    await exportCephJournal(journalCsv(filtered.value))
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    exporting.value = false
  }
}
</script>

<template>
  <WorkspacePanel title="Replication journals" description="Read a metadata, data or bucket-index journal from its source gateway.">
    <form class="journal-content" @submit.prevent="scope && load(scope)">
      <fieldset class="journal-fields" :disabled="busy">
        <label>Journal<AppSelect v-model="input.family" :options="families" :disabled="busy" /></label>
        <label>Source gateway<AppSelect v-model="input.gateway" :options="endpoints" placeholder="Select a verified gateway" :disabled="busy" @update:model-value="resetBucketBinding" /></label>
        <label v-if="input.family === 'metadata'">Period ID<input v-model.trim="input.period" required /></label>
        <template v-if="input.family === 'bilog'">
          <label>Bucket name<input v-model.trim="input.bucket.name" required @input="resetBucketBinding" /></label>
          <label>Bucket tenant<input v-model.trim="input.bucket.tenant" placeholder="Default tenant" @input="resetBucketBinding" /></label>
          <label>Generation<input v-model.trim="input.generation" inputmode="numeric" required /></label>
        </template>
        <label>Shard <span v-if="input.family === 'bilog'" class="muted">(optional)</span>
          <input v-model.trim="input.shard" inputmode="numeric" :required="input.family !== 'bilog'" />
        </label>
      </fieldset>
      <p v-if="input.family === 'bilog' && input.bucket.instance" class="muted">Verified bucket instance {{ input.bucket.instance }}</p>
      <div class="panel-actions"><button class="primary" :disabled="!scope || busy">{{ busy ? 'Reading journal…' : 'Read journal' }}</button></div>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="unavailable" tone="error">
        {{ availabilityLabels[unavailable.availability] }}<template v-if="unavailable.status"> · HTTP {{ unavailable.status }}</template>
        <template v-if="unavailable.availability === 'stale'">. Refresh the topology and verify the journal scope.</template>
      </Notice>
    </form>
  </WorkspacePanel>
  <WorkspacePanel v-if="loaded" title="Loaded journal rows" :description="loadedScopeLabel">
    <div class="journal-content">
      <div class="journal-tools">
        <input v-model="filter" type="search" placeholder="Filter loaded rows" aria-label="Filter loaded journal rows" />
        <button type="button" :disabled="!filtered.length || exporting" @click="exportRows"><Download :size="15" /> Export loaded rows</button>
      </div>
      <p role="status" class="muted">{{ filtered.length }} of {{ loaded.entries.length }} loaded rows<template v-if="loaded.lastUpdate"> · Updated {{ formatDate(loaded.lastUpdate, true) }}</template></p>
      <div v-if="filtered.length" class="journal-table">
        <table class="report-table">
          <thead><tr><th scope="col">Timestamp</th><th scope="col">Key</th><th scope="col">Operation</th><th scope="col">Category</th><th scope="col">ID</th></tr></thead>
          <tbody><tr v-for="(entry, index) in filtered" :key="index">
            <td>{{ formatDate(entry.timestamp, true) }}</td>
            <td>{{ entry.key || '—' }}<small v-if="entry.instance" class="muted">{{ entry.instance }}</small></td>
            <td>{{ entry.operation || '—' }}</td><td>{{ entry.category || '—' }}</td><td>{{ entry.id || '—' }}</td>
          </tr></tbody>
        </table>
      </div>
      <p v-else class="muted">{{ loaded.entries.length ? 'No loaded rows match the filter.' : 'This journal page contains no entries.' }}</p>
      <div class="panel-actions">
        <button v-if="nextPage" type="button" :disabled="busy" @click="load(nextPage, true)">Load next page</button>
        <button v-if="nextGeneration && nextGeneration.journal.kind === 'bilog'" type="button" :disabled="busy" @click="loadGeneration">Open generation {{ nextGeneration.journal.generation }}</button>
        <span v-else-if="loaded.nextGeneration && !nextPage" class="muted">Generation {{ loaded.nextGeneration.generation }} has {{ loaded.nextGeneration.shards }} shards. Select an available shard to continue.</span>
      </div>
    </div>
  </WorkspacePanel>
</template>

<style scoped>
.journal-content { display: grid; gap: 12px; padding: 14px 16px; }
.journal-fields { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(180px, 100%), 1fr)); gap: 10px; min-width: 0; margin: 0; padding: 0; border: 0; }
.journal-fields label { display: grid; align-content: end; gap: 5px; }
.journal-tools { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; }
.journal-tools input { flex: 1 1 170px; max-width: 340px; }
.journal-table { overflow: auto; }
.journal-table td { overflow-wrap: anywhere; }
.journal-table small { display: block; }
</style>
