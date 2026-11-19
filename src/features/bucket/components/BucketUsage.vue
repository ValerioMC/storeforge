<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatDate, formatSize } from '../../../common/format'
import AppSelect from '../../../components/ui/AppSelect.vue'
import CoverageLabel from '../../monitoring/components/CoverageLabel.vue'
import { formatCount } from '../../monitoring/model/dashboard'
import type { BucketSample } from '../../monitoring/model/monitoring'

const props = defineProps<{ sample?: BucketSample; pending: boolean; stale: boolean; demo: boolean }>()
defineEmits<{ collect: [] }>()
const distribution = ref<'sizes' | 'ages' | 'types' | 'storageClasses'>('storageClasses')
const distributions = [
  { value: 'storageClasses', label: 'Storage class' }, { value: 'types', label: 'Data type' },
  { value: 'sizes', label: 'Object size' }, { value: 'ages', label: 'Object age' },
] as const
const usage = computed(() => props.sample?.usage.value)
const bins = computed(() => Object.entries(usage.value?.[distribution.value] ?? {})
  .sort((left, right) => (right[1].bytes ?? -1) - (left[1].bytes ?? -1)))
</script>

<template>
  <div class="workspace-drawer-content bucket-usage">
    <div class="bucket-detail-heading">
      <div>
        <h3>Storage usage</h3>
        <p class="muted">{{ sample ? `Collected ${formatDate(sample.collectedAt, true)}` : 'No usage sample has been collected for this bucket.' }}</p>
      </div>
      <div class="panel-actions">
        <CoverageLabel v-if="sample" :coverage="sample.usage.coverage" :detail="sample.usage.detail" :stale="stale" />
        <button v-if="!demo" :disabled="pending" @click="$emit('collect')">{{ pending ? 'Collecting…' : 'Collect usage' }}</button>
      </div>
    </div>
    <template v-if="usage">
      <dl class="setup-summary">
        <div><dt>Current storage</dt><dd>{{ formatSize(usage.currentBytes) }}</dd></div>
        <div><dt>Current objects</dt><dd>{{ formatCount(usage.currentObjects) }}</dd></div>
        <div><dt>Noncurrent storage</dt><dd>{{ formatSize(usage.noncurrentBytes) }}</dd></div>
        <div><dt>Noncurrent objects</dt><dd>{{ formatCount(usage.noncurrentObjects) }}</dd></div>
        <div><dt>Delete markers</dt><dd>{{ formatCount(usage.deleteMarkers) }}</dd></div>
      </dl>
      <div class="bucket-detail-heading">
        <h3>Object distribution</h3><AppSelect v-model="distribution" :options="distributions" aria-label="Object distribution" />
      </div>
      <div v-if="bins.length" class="workspace-table">
        <table>
          <thead><tr><th scope="col">Group</th><th scope="col">Objects</th><th scope="col">Storage</th></tr></thead>
          <tbody><tr v-for="[name, reading] in bins" :key="name"><th scope="row">{{ name }}</th><td>{{ formatCount(reading.objects) }}</td><td>{{ formatSize(reading.bytes) }}</td></tr></tbody>
        </table>
      </div>
      <p v-else class="muted">No objects in this distribution.</p>
    </template>
  </div>
</template>

<style scoped>
.bucket-usage { display: grid; gap: 14px; padding: 16px; }
.bucket-usage .setup-summary { margin: 0; }
</style>
