<script setup lang="ts">
import { ChevronRight } from 'lucide-vue-next'
import { formatSize } from '../../../common/format'
import { formatCount, quotaFraction } from '../model/dashboard'
import type { BucketSample } from '../model/monitoring'
import CoverageLabel from './CoverageLabel.vue'

defineProps<{ buckets: BucketSample[]; quotas?: boolean }>()
defineEmits<{ bucket: [name: string] }>()

function quotaText(used: number | null | undefined, maximum: number | null, format: typeof formatSize): string {
  const fraction = quotaFraction(used, maximum)
  if (maximum === null) return 'Unlimited'
  return fraction === null ? format(maximum) : `${format(used)} / ${format(maximum)}`
}
</script>

<template>
  <div v-if="buckets.length" class="workspace-table monitoring-table">
    <table>
      <thead><tr><th scope="col">Bucket</th><th scope="col">{{ quotas ? 'Storage limit' : 'Current storage' }}</th>
        <th scope="col">{{ quotas ? 'Object limit' : 'Objects' }}</th><th scope="col">Coverage</th></tr></thead>
      <tbody>
        <tr v-for="item in buckets" :key="item.bucket">
          <th scope="row"><button class="subtle bucket-record" @click="$emit('bucket', item.bucket)">{{ item.bucket }} <ChevronRight :size="13" /></button></th>
          <template v-if="quotas">
            <td v-if="item.quota.value?.enabled">
              {{ quotaText(item.rgwStorage.value?.bytes, item.quota.value.maxBytes, formatSize) }}
              <meter v-if="quotaFraction(item.rgwStorage.value?.bytes, item.quota.value.maxBytes) != null"
                min="0" max="1" :value="Math.min(1, quotaFraction(item.rgwStorage.value?.bytes, item.quota.value.maxBytes) ?? 0)" :aria-label="`${item.bucket} storage quota`" />
            </td>
            <td v-if="item.quota.value?.enabled">
              {{ quotaText(item.rgwStorage.value?.objects, item.quota.value.maxObjects, formatCount) }}
              <meter v-if="quotaFraction(item.rgwStorage.value?.objects, item.quota.value.maxObjects) != null"
                min="0" max="1" :value="Math.min(1, quotaFraction(item.rgwStorage.value?.objects, item.quota.value.maxObjects) ?? 0)" :aria-label="`${item.bucket} object quota`" />
            </td>
            <td v-else colspan="2">{{ item.quota.value ? 'Disabled' : '—' }}</td>
            <td><CoverageLabel :coverage="item.quota.coverage" :detail="item.quota.detail" /></td>
          </template>
          <template v-else>
            <td>{{ formatSize(item.usage.value?.currentBytes) }}</td><td>{{ formatCount(item.usage.value?.currentObjects) }}</td>
            <td><CoverageLabel :coverage="item.usage.coverage" :detail="item.usage.detail" /></td>
          </template>
        </tr>
      </tbody>
    </table>
  </div>
  <p v-else class="workspace-panel-message">No bucket measurements to show.</p>
</template>
