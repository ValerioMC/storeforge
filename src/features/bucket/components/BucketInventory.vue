<script setup lang="ts">
import { computed, ref } from 'vue'
import { ChevronRight, Database, Settings2 } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Notice from '../../../components/ui/Notice.vue'
import { formatCount } from '../../monitoring/model/dashboard'
import type { BucketSample } from '../../monitoring/model/monitoring'
import { useConfigurationInventory } from '../composables/useConfigurationInventory'
import type { Bucket, Configuration } from '../model/bucket'
import { configurations } from '../model/configuration'

const props = defineProps<{
  connection: string; buckets: Bucket[]; samples: BucketSample[]; loading: boolean; error: string
}>()
const emit = defineEmits<{ inspect: [name: string]; connections: [] }>()
const search = ref('')
const columns = ref<Configuration[]>([])
const columnsOpen = ref(false)
const feature = ref<Configuration | ''>('')
const status = ref<'all' | 'configured' | 'missing' | 'unsupported' | 'error'>('all')
const statusOptions = [
  { value: 'all', label: 'Any state' }, { value: 'configured', label: 'Configured' },
  { value: 'missing', label: 'Not configured' }, { value: 'unsupported', label: 'Unsupported' }, { value: 'error', label: 'Read error' },
] as const
const featureOptions = [{ value: '', label: 'All features' }, ...configurations.map(({ id, label }) => ({ value: id, label }))]
const shownColumns = computed(() => configurations.filter(({ id }) => columns.value.includes(id) || id === feature.value))
const features = computed(() => shownColumns.value.map(({ id }) => id))
const candidates = computed(() => props.buckets.filter(({ name }) =>
  name.toLowerCase().includes(search.value.trim().toLowerCase())))
const names = computed(() => candidates.value.map(({ name }) => name))
const { rows, pending, refresh, invalidate } = useConfigurationInventory(computed(() => props.connection), names, features)
const matching = computed(() => candidates.value.filter(({ name }) => !feature.value || status.value === 'all' || rows.get(name)?.[feature.value]?.status === status.value))
const sampleByBucket = computed(() => new Map(props.samples.map(sample => [sample.bucket, sample])))

function clearFilters(): void {
  search.value = ''
  feature.value = ''
  status.value = 'all'
}

defineExpose({ refresh, invalidate })
</script>

<template>
  <WorkspacePanel title="Bucket inventory" :description="`${matching.length.toLocaleString()} of ${buckets.length.toLocaleString()} buckets`">
    <template #actions>
      <button type="button" :aria-expanded="columnsOpen" @click="columnsOpen = !columnsOpen"><Settings2 :size="15" /> Columns</button>
    </template>
    <div class="bucket-table-controls">
      <label><span class="sr-only">Find a bucket</span><input v-model="search" type="search" placeholder="Find a bucket" /></label>
      <AppSelect v-model="feature" :options="featureOptions" aria-label="Filter by bucket feature" />
      <AppSelect v-if="feature" v-model="status" :options="statusOptions" aria-label="Filter by configuration state" />
    </div>
    <div v-if="columnsOpen" class="bucket-column-options" aria-label="Feature columns">
      <label v-for="{ id, label } in configurations" :key="id" class="check-label">
        <input v-if="id === feature" type="checkbox" checked disabled />
        <input v-else v-model="columns" type="checkbox" :value="id" /> {{ label }}
      </label>
    </div>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <p v-else-if="loading || pending" class="workspace-panel-message" role="status">{{ loading ? 'Reading buckets…' : 'Reading selected features…' }}</p>
    <div v-if="matching.length && !error" class="workspace-table">
      <table>
        <thead>
          <tr>
            <th scope="col">Bucket</th><th scope="col">Current storage</th><th scope="col">Objects</th><th scope="col">Created</th>
            <th v-for="{ id, label } in shownColumns" :key="id" scope="col">{{ label }}</th>
            <th scope="col"><span class="sr-only">Inspect</span></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="item in matching" :key="item.name">
            <th scope="row"><button class="subtle bucket-record" @click="emit('inspect', item.name)"><Database :size="16" />{{ item.name }}</button></th>
            <td>{{ formatSize(sampleByBucket.get(item.name)?.usage.value?.currentBytes) }}</td>
            <td>{{ formatCount(sampleByBucket.get(item.name)?.usage.value?.currentObjects) }}</td>
            <td>{{ formatDate(item.createdAt) }}</td>
            <td v-for="{ id } in shownColumns" :key="id">
              <template v-for="reading in [rows.get(item.name)?.[id]]" :key="id">
                <span v-if="reading?.status === 'error'" :title="reading.message">Read error</span>
                <span v-else-if="reading" :title="reading.status === 'unsupported' ? reading.message : undefined">{{ reading.label }}</span>
                <span v-else class="muted">—</span>
              </template>
            </td>
            <td><button class="subtle icon-button" :aria-label="`Inspect ${item.name}`" @click="emit('inspect', item.name)"><ChevronRight :size="16" /></button></td>
          </tr>
        </tbody>
      </table>
    </div>
    <EmptyState v-else-if="!error && !loading && !pending" :icon="Database" :title="buckets.length ? 'No matching buckets' : 'No visible buckets'"
      :description="buckets.length ? undefined : 'Create a bucket or review the known buckets in Connections.'">
      <button v-if="search || feature" @click="clearFilters">Clear filters</button>
      <button v-else-if="connection !== 'demo' && !buckets.length" @click="emit('connections')">Manage connections</button>
    </EmptyState>
  </WorkspacePanel>
</template>
