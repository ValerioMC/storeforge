<script setup lang="ts">
import { formatDate } from '../../../common/format'
import Notice from '../../../components/ui/Notice.vue'
import { availabilityLabels, type Observation, type SyncStatus } from '../model/ceph-multisite'

defineProps<{ value: Observation<SyncStatus> }>()
</script>

<template>
  <Notice v-if="!value.data" :tone="value.availability === 'notApplicable' ? 'info' : 'error'">
    {{ availabilityLabels[value.availability] }}<template v-if="value.status"> · HTTP {{ value.status }}</template>
    <template v-if="value.availability === 'stale'">. Refresh topology and verify the selected gateway.</template>
  </Notice>
  <template v-else>
    <dl class="setup-summary">
      <div><dt>Phase</dt><dd>{{ value.data.phase }}</dd></div>
      <div><dt>Observed shards</dt><dd>{{ value.data.shards.length }}<template v-if="value.data.expectedShards !== null"> of {{ value.data.expectedShards }}</template></dd></div>
      <div v-if="value.data.period"><dt>Sync period</dt><dd>{{ value.data.period }}</dd></div>
      <div v-if="value.data.generation !== null"><dt>Generation</dt><dd>{{ value.data.generation }}</dd></div>
    </dl>
    <div v-if="value.data.shards.length" class="shard-table">
      <table class="report-table">
        <thead><tr><th scope="col">Shard</th><th scope="col">Phase</th><th scope="col">Position</th><th scope="col">Timestamp</th></tr></thead>
        <tbody>
          <tr v-for="shard in value.data.shards" :key="shard.id">
            <th scope="row">{{ shard.id }}</th><td>{{ shard.phase }}</td><td>{{ shard.marker || '—' }}</td><td>{{ formatDate(shard.timestamp) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </template>
</template>

<style scoped>
.shard-table { overflow: auto; }
.shard-table td { overflow-wrap: anywhere; }
</style>
