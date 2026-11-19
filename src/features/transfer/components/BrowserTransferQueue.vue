<script setup lang="ts">
import { computed } from 'vue'
import { Activity } from 'lucide-vue-next'
import { useJobs } from '../store/jobs'
import TransferJobDetail from './TransferJobDetail.vue'

const props = defineProps<{ operationIds: readonly string[] }>()
const store = useJobs()
const jobs = computed(() => store.jobs.filter(job => props.operationIds.includes(job.id)))
const active = computed(() => jobs.value.filter(job => job.status === 'running').length)
</script>

<template>
  <details v-if="jobs.length" class="browser-queue settings-sheet">
    <summary><Activity :size="15" /> Transfers <span class="muted">{{ active ? active + ' running' : jobs.length + ' recent' }}</span></summary>
    <div class="queue-content">
      <details v-for="job in jobs" :key="job.id" class="job">
        <summary><strong>{{ job.label }}</strong><span class="muted">{{ job.succeeded + job.failed }} / {{ job.total }}</span><span class="chip" :class="'job-' + job.status">{{ job.status }}</span></summary>
        <TransferJobDetail :job="job" />
      </details>
    </div>
  </details>
</template>

<style scoped>
.browser-queue { margin-block: 8px; }
.browser-queue > summary { display: flex; align-items: center; gap: 8px; cursor: pointer; padding: 8px 12px; font-weight: 640; }
.browser-queue > summary .muted { margin-inline-start: auto; font-weight: 400; }
.queue-content { max-height: 340px; overflow: auto; }
</style>
