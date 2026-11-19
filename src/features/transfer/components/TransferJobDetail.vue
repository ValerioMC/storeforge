<script setup lang="ts">
import { ref } from 'vue'
import { CircleStop } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import { formatDate, formatSize } from '../../../common/format'
import Notice from '../../../components/ui/Notice.vue'
import { transfersApi } from '../api/transfers'
import type { Job } from '../model/transfer'
import { useJobs } from '../store/jobs'

const props = defineProps<{ job: Job; demo?: boolean }>()
const store = useJobs()
const failure = ref('')
const limit = ref(25)

async function run(action: () => Promise<void>): Promise<void> {
  failure.value = ''
  try {
    await action()
  } catch (error) {
    failure.value = errorMessage(error)
  }
}

async function exportDetails(): Promise<void> {
  await run(() => transfersApi.exportReport('operation-' + props.job.id + '.json', JSON.stringify(props.job, null, 2)))
}
</script>

<template>
  <div class="job-detail">
    <progress v-if="job.total > 0" :class="{ failed: job.failed > 0 }" :max="job.total" :value="Math.min(job.total, job.succeeded + job.failed)" :aria-label="job.label + ' progress'" />
    <progress v-else-if="job.status === 'running'" aria-label="Preparing operation" />
    <Notice v-if="failure" tone="error">{{ failure }}</Notice>
    <div class="inline between job-actions">
      <p>{{ job.succeeded }} succeeded · {{ job.failed }} failed<span v-if="job.bytesProcessed"> · {{ formatSize(job.bytesProcessed) }} transferred</span>
        <span v-if="job.finishedAt"> · {{ formatDate(job.finishedAt, true) }}</span></p>
      <div class="inline job-actions">
        <button v-if="job.status === 'running'" :disabled="job.cancelRequested || demo" @click="run(() => store.cancel(job.id))">
          <CircleStop :size="16" />{{ job.cancelRequested ? 'Stopping…' : 'Stop after current item' }}
        </button>
        <button :disabled="demo" @click="exportDetails">Export JSON</button>
      </div>
    </div>
    <p v-if="job.current" class="muted">Current item <code>{{ job.current }}</code><span v-if="job.currentBytes"> · {{ formatSize(job.currentBytes) }} streamed</span></p>
    <ul v-if="job.failures.length" class="failure-list"><li v-for="(item, index) in job.failures.slice(0, limit)" :key="index">
      <code>{{ item.item }}</code><span>{{ item.message }}</span>
    </li></ul>
    <details v-if="job.successfulItems?.length"><summary>{{ job.successfulItems.length }} successful {{ job.successfulItems.length === 1 ? 'item' : 'items' }}</summary>
      <ul class="failure-list"><li v-for="(item, index) in job.successfulItems.slice(0, limit)" :key="index"><code>{{ item }}</code></li></ul>
    </details>
    <button v-if="Math.max(job.failures.length, job.successfulItems?.length ?? 0) > limit" @click="limit += 25">Show more results</button>
  </div>
</template>

<style scoped>
.job-actions { flex-wrap: wrap; }
code { white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
