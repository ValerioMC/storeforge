<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Activity, ArrowDownToLine, ArrowUpFromLine, ClipboardList, MoveRight } from 'lucide-vue-next'
import { formatDate } from '../../../common/format'
import AuditHistory from '../../audit/components/AuditHistory.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { Job, JobStatus } from '../model/transfer'
import TransferJobDetail from './TransferJobDetail.vue'
import { useJobs } from '../store/jobs'

const props = defineProps<{ demo: boolean; preselected: string | null; audit?: boolean }>()
const emit = defineEmits<{ browse: [] }>()
const store = useJobs()
const view = ref<'Jobs' | 'Audit'>('Jobs')
watch(() => props.audit, audit => { view.value = audit ? 'Audit' : 'Jobs' }, { immediate: true })
const filter = ref<'all' | JobStatus>('all')

const demoJobs: Job[] = [
  { id: 'demo-1', label: 'Upload', status: 'completed', startedAt: '2026-11-18T09:24:00Z', finishedAt: '2026-11-18T09:25:00Z', total: 12, succeeded: 12, failed: 0, failures: [], current: null, cancelRequested: false },
  { id: 'demo-2', label: 'Download ZIP', status: 'completed', startedAt: '2026-11-17T17:10:00Z', finishedAt: '2026-11-17T17:14:00Z', total: 6, succeeded: 6, failed: 0, failures: [], current: null, cancelRequested: false },
  { id: 'demo-3', label: 'Copy objects', status: 'failed', startedAt: '2026-11-17T16:20:00Z', finishedAt: '2026-11-17T16:21:00Z', total: 4, succeeded: 3, failed: 1, failures: [{ item: 'telemetry/pass-017/contacts.csv', message: 'The destination already contains this key.' }], current: null, cancelRequested: false },
]
const all = computed(() => props.demo ? demoJobs : store.jobs)
const visible = computed(() => filter.value === 'all' ? all.value : all.value.filter(job => job.status === filter.value))
const groups = computed(() => ([
  { id: 'all', label: 'All', count: all.value.length },
  { id: 'running', label: 'Running', count: all.value.filter(job => job.status === 'running').length },
  { id: 'completed', label: 'Completed', count: all.value.filter(job => job.status === 'completed').length },
  { id: 'failed', label: 'Failed', count: all.value.filter(job => job.status === 'failed').length },
] as const))

function statusLabel(status: JobStatus): string {
  return status.slice(0, 1).toUpperCase() + status.slice(1)
}

function jobIcon(label: string) {
  if (label === 'Upload') return ArrowUpFromLine
  if (label.startsWith('Download')) return ArrowDownToLine
  if (label.startsWith('Copy') || label.startsWith('Move')) return MoveRight
  return ClipboardList
}

</script>

<template>
  <section class="workspace-page">
    <div class="page-header"><div><h1>Activity</h1><p>Follow transfers and bulk jobs, with results for individual failures.</p></div><span class="context-chip"><Activity :size="17" /> {{ props.demo ? 'Demo activity' : 'This device' }}</span></div>
    <nav class="workspace-tabs" aria-label="Activity views">
      <button v-for="option in (['Jobs', 'Audit'] as const)" :key="option" :class="{ active: view === option }" :aria-pressed="view === option" @click="view = option">{{ option }}</button>
    </nav>
    <FadeRegion>
      <div v-if="view === 'Audit'"><AuditHistory :demo="demo" /></div>
      <div v-else>
        <nav class="workspace-tabs activity-filters" aria-label="Filter activity">
          <button v-for="group in groups" :key="group.id" :class="{ active: group.id === filter }" :aria-pressed="group.id === filter" @click="filter = group.id">{{ group.label }} <span class="tab-count">{{ group.count }}</span></button>
        </nav>
        <Notice v-if="store.error" tone="error">{{ store.error }}</Notice>
        <EmptyState v-if="!visible.length" :icon="Activity" :title="filter === 'all' ? 'No transfer activity' : 'No matching operations'" description="Uploads, downloads, migrations, and bulk changes appear here.">
          <button @click="emit('browse')">Open object browser</button>
        </EmptyState>
        <div v-else class="settings-sheet job-list">
          <details v-for="job in visible" :key="job.id" class="job" :open="job.id === preselected || job.status === 'running' || undefined">
            <summary>
              <span class="file-icon"><component :is="jobIcon(job.label)" :size="18" /></span>
              <span class="settings-description"><strong>{{ job.label }}</strong><small>{{ formatDate(job.startedAt, true) }}</small></span>
              <span class="job-count">{{ Math.min(job.total, job.succeeded + job.failed) }} / {{ job.total }} processed</span>
              <span class="chip" :class="`job-${job.status}`">{{ statusLabel(job.status) }}</span>
            </summary>
            <TransferJobDetail :job="job" :demo="demo" />
          </details>
        </div>
      </div>
    </FadeRegion>
  </section>
</template>
