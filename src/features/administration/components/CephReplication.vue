<script setup lang="ts">
import { computed, ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { readMultisiteReplication } from '../api/ceph-multisite'
import { availabilityLabels, type BucketIdentity, type BucketSelector, type Replication, type ZoneBinding } from '../model/ceph-multisite'
import CephSyncObservation from './CephSyncObservation.vue'

const props = defineProps<{ endpoints: { label: string; value: string; binding: ZoneBinding }[] }>()
const emit = defineEmits<{ journal: [gateway: string, bucket: BucketIdentity, generation: string] }>()
const source = ref('')
const destination = ref('')
const sourceBucket = ref<BucketSelector>({ name: '', tenant: '', instance: null })
const destinationBucket = ref<BucketSelector>({ name: '', tenant: '', instance: null })
const result = ref<Replication>()
const observedSource = ref('')
const error = ref('')
const busy = ref(false)
const sourceBinding = computed(() => props.endpoints.find(item => item.value === source.value)?.binding)
const destinationOptions = computed(() => props.endpoints.filter(item => item.binding.zone !== sourceBinding.value?.zone))
const destinationBinding = computed(() => destinationOptions.value.find(item => item.value === destination.value)?.binding)
const ready = computed(() => sourceBinding.value && destinationBinding.value && sourceBucket.value.name && destinationBucket.value.name)
const bucketReadings = computed(() => result.value ? [
  { label: 'Source bucket', value: result.value.source }, { label: 'Destination bucket', value: result.value.destination },
] : [])

function selectSource(): void {
  if (!destinationBinding.value) destination.value = ''
}

async function read(): Promise<void> {
  const from = sourceBinding.value
  const to = destinationBinding.value
  if (!from || !to || !ready.value || busy.value) return
  busy.value = true
  result.value = undefined
  error.value = ''
  observedSource.value = source.value
  try {
    result.value = await readMultisiteReplication({ source: from, destination: to,
      sourceBucket: { ...sourceBucket.value }, destinationBucket: { ...destinationBucket.value } })
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}

function browseJournal(generation: string): void {
  const bucket = result.value?.source.data
  if (bucket) emit('journal', observedSource.value, bucket, generation)
}
</script>

<template>
  <WorkspacePanel title="Bucket replication" description="Read sync state between two verified zones.">
    <form class="multisite-content" @submit.prevent="read">
      <fieldset class="replication-scope" :disabled="busy">
        <div class="replication-fields">
          <label>Source gateway<AppSelect v-model="source" :options="endpoints" placeholder="Select source gateway" :disabled="busy" @update:model-value="selectSource" /></label>
          <label>Source bucket<input v-model.trim="sourceBucket.name" required /></label>
          <label>Source tenant<input v-model.trim="sourceBucket.tenant" placeholder="Default tenant" /></label>
        </div>
        <div class="replication-fields">
          <label>Destination gateway<AppSelect v-model="destination" :options="destinationOptions" placeholder="Select destination gateway" :disabled="busy" /></label>
          <label>Destination bucket<input v-model.trim="destinationBucket.name" required /></label>
          <label>Destination tenant<input v-model.trim="destinationBucket.tenant" placeholder="Default tenant" /></label>
        </div>
      </fieldset>
      <div class="panel-actions"><button class="primary" :disabled="!ready || busy">{{ busy ? 'Reading replication…' : 'Read replication status' }}</button></div>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
    </form>
  </WorkspacePanel>
  <WorkspacePanel v-if="result" title="Replication status">
    <div class="multisite-content">
      <dl class="setup-summary">
        <div v-for="{ label, value } in bucketReadings" :key="label">
          <dt>{{ label }}</dt>
          <dd v-if="value.data">{{ value.data.tenant || 'Default tenant' }} · {{ value.data.name }} · {{ value.data.instance }}</dd>
          <dd v-else>{{ availabilityLabels[value.availability] }}<template v-if="value.status"> · HTTP {{ value.status }}</template></dd>
        </div>
      </dl>
      <CephSyncObservation :value="result.sync" />
    </div>
  </WorkspacePanel>
  <WorkspacePanel v-if="result" title="Source bucket index generations">
    <div class="multisite-content">
      <Notice v-if="!result.bilog.data" tone="error">{{ availabilityLabels[result.bilog.availability] }}<template v-if="result.bilog.status"> · HTTP {{ result.bilog.status }}</template></Notice>
      <template v-else>
        <p class="muted">Oldest {{ result.bilog.data.oldest ?? 'unavailable' }} · Latest {{ result.bilog.data.latest ?? 'unavailable' }}</p>
        <div v-if="result.bilog.data.generations.length" class="generation-table">
          <table class="report-table">
            <thead><tr><th scope="col">Generation</th><th scope="col">Shards</th><th scope="col">Journal</th></tr></thead>
            <tbody><tr v-for="entry in result.bilog.data.generations" :key="entry.generation">
              <th scope="row">{{ entry.generation }}</th><td>{{ entry.shards }}</td>
              <td><button type="button" @click="browseJournal(entry.generation)">Open journal</button></td>
            </tr></tbody>
          </table>
        </div>
        <p v-else class="muted">The gateway returned no generations.</p>
      </template>
    </div>
  </WorkspacePanel>
</template>

<style scoped>
.multisite-content { display: grid; gap: 12px; padding: 14px 16px; }
.replication-scope { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 20px; min-width: 0; margin: 0; padding: 0; border: 0; }
.replication-fields, .replication-fields label { display: grid; gap: 5px; min-width: 0; }
.replication-fields { gap: 10px; }
.generation-table { overflow: auto; }
@media (max-width: 720px) { .replication-scope { grid-template-columns: 1fr; } }
</style>
