<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ClipboardCheck, RefreshCw, Search } from 'lucide-vue-next'
import { formatDate } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Notice from '../../../components/ui/Notice.vue'
import { useConnections } from '../../connection/store/connections'
import { listAudit } from '../api/audit'
import type { AuditEvent } from '../model/audit'

const props = defineProps<{ demo: boolean }>()
const connections = useConnections()
const events = ref<AuditEvent[]>([])
const query = ref('')
const error = ref('')
const busy = ref(false)
const visible = computed(() => {
  const filter = query.value.toLowerCase()
  return events.value.filter(event => [event.action, event.resource ?? '', connectionName(event.connection)].some(value => value.toLowerCase().includes(filter)))
})

onMounted(() => void load())

function connectionName(id: string): string {
  if (id === 'demo') return 'Asteria Research'
  return connections.profiles.find(profile => profile.id === id)?.name ?? 'Removed connection'
}

async function load(): Promise<void> {
  if (props.demo) {
    events.value = [
      { id: '1', timestamp: '2026-11-18T09:24:00Z', connection: 'demo', action: 'Change object tags', resource: 'mission-archive/telemetry/pass-018/pass-018-summary.csv', success: true },
      { id: '2', timestamp: '2026-11-17T17:25:00Z', connection: 'demo', action: 'Inspect bucket statistics', resource: null, success: true },
      { id: '3', timestamp: '2026-11-17T17:19:00Z', connection: 'demo', action: 'Change bucket settings', resource: 'mission-archive', success: false },
    ]
    return
  }
  busy.value = true
  error.value = ''
  try {
    events.value = await listAudit()
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="inline between audit-controls">
    <label class="object-search"><Search :size="16" /><input v-model="query" type="search" aria-label="Filter local audit history" placeholder="Filter local history" /></label>
    <button :disabled="busy" @click="load"><RefreshCw :size="16" /> Refresh history</button>
  </div>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <p v-if="busy" class="loading-line" role="status">Loading audit history…</p>
  <EmptyState v-else-if="!visible.length" :icon="ClipboardCheck" :title="query ? 'No matching actions' : 'No audit entries yet'" description="Bucket, object and administration requests made from this desktop appear here." />
  <div v-else class="settings-sheet">
    <div v-for="event in visible" :key="event.id" class="settings-row">
      <span class="file-icon"><ClipboardCheck :size="16" /></span>
      <div class="settings-description"><strong>{{ event.action }}</strong><p>{{ connectionName(event.connection) }}<span v-if="event.resource"> · <code>{{ event.resource }}</code></span></p></div>
      <small class="muted">{{ formatDate(event.timestamp, true) }}</small>
      <span class="chip" :class="event.success ? 'job-completed' : 'job-failed'">{{ event.success ? 'Accepted' : 'Failed' }}</span>
    </div>
  </div>
</template>
