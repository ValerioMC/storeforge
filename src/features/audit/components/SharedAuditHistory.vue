<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ClipboardCheck, RefreshCw } from 'lucide-vue-next'
import AppSelect from '../../../components/ui/AppSelect.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { formatDate } from '../../../common/format'
import QueryPager from '../../administration/components/QueryPager.vue'
import { sharedServiceApi } from '../../connection/api/shared-service'
import type { SharedContext } from '../../connection/model/shared-service'
import { operationsApi } from '../api/operational-events'
import { eventLabel, type EventStatus, type HistoryFilter, type HistoryScope, type ServiceAuditPage } from '../model/operational-events'

const props = defineProps<{ serviceId: string; scope: HistoryScope }>()
const statusOptions: { value: EventStatus | ''; label: string }[] = [
  { value: '', label: 'All statuses' }, { value: 'success', label: 'Success' },
  { value: 'failure', label: 'Failure' }, { value: 'partial', label: 'Partial' },
]
const page = ref<ServiceAuditPage>()
const context = ref('')
const contexts = ref<SharedContext[]>([])
const contextOptions = computed(() => contexts.value.map(item => ({ value: item.id, label: item.name })))
const search = ref('')
const actor = ref('')
const status = ref<EventStatus | ''>('')
const filters = ref<HistoryFilter>({})
const cursors = ref<(number | undefined)[]>([undefined])
const pending = ref(false)
const error = ref('')

onMounted(initialize)

async function initialize(): Promise<void> {
  if (props.scope === 'admin') {
    await load(undefined)
    return
  }
  pending.value = true
  try {
    contexts.value = await sharedServiceApi.contexts(props.serviceId, 'manager')
    if (contexts.value.length === 1 && contexts.value[0]) chooseContext(contexts.value[0].id)
    else pending.value = false
  } catch (failure) {
    error.value = errorMessage(failure)
    pending.value = false
  }
}

function chooseContext(value: string): void {
  context.value = value
  applyFilters()
}

function applyFilters(): void {
  filters.value = { context_id: context.value || undefined, search: search.value.trim() || undefined,
    actor: actor.value.trim() || undefined, status: status.value || undefined }
  cursors.value = [undefined]
  void load(undefined)
}

function next(): void {
  const cursor = page.value?.next_cursor
  if (cursor == null) return
  cursors.value.push(cursor)
  void load(cursor)
}

function previous(): void {
  cursors.value.pop()
  void load(cursors.value.at(-1))
}

async function load(cursor: number | undefined): Promise<void> {
  if (props.scope === 'manager' && !context.value) return
  pending.value = true
  page.value = undefined
  error.value = ''
  try {
    page.value = await operationsApi.history(props.serviceId, props.scope, { ...filters.value, cursor })
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <form class="settings-toolbar history-filters" @submit.prevent="applyFilters">
    <AppSelect v-if="scope === 'manager'" :model-value="context" :options="contextOptions" aria-label="Manager activity context"
      placeholder="Choose a context" :disabled="pending" @update:model-value="chooseContext" />
    <input v-else v-model="context" type="search" aria-label="Execution context ID" placeholder="Context ID" />
    <input v-model="search" type="search" aria-label="Search service activity" placeholder="Search events" maxlength="200" />
    <input v-model="actor" type="search" aria-label="Actor email" placeholder="Actor email" />
    <AppSelect v-model="status" :options="statusOptions" aria-label="Event status" :disabled="pending" />
    <button type="submit" :disabled="pending || (scope === 'manager' && !context)">Apply filters</button>
    <button type="button" :disabled="pending || (scope === 'manager' && !context)" @click="load(cursors.at(-1))"><RefreshCw :size="15" /> Refresh</button>
  </form>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <p v-if="pending" role="status">Reading service events…</p>
  <template v-else-if="!error">
    <div v-if="page?.logs.length" class="settings-sheet">
      <details v-for="event in page.logs" :key="event.id">
        <summary class="settings-row">
          <div class="settings-description">
            <strong>{{ eventLabel(event.action) }}</strong>
            <p>{{ event.user_email }}<template v-if="event.context_name"> · {{ event.context_name }}</template></p>
          </div>
          <small>{{ formatDate(event.created_at, true) }}</small>
          <span class="chip" :class="event.status === 'failure' ? 'job-failed' : event.status === 'success' ? 'job-completed' : ''">{{ eventLabel(event.status) }}</span>
        </summary>
        <div class="event-detail">
          <p v-if="event.entity_type || event.entity_id">{{ eventLabel(event.entity_type ?? '') }} <code>{{ event.entity_id }}</code></p>
          <p v-if="event.scope || event.user_role">{{ [event.scope, event.user_role].filter(Boolean).map(value => eventLabel(value ?? '')).join(' · ') }}</p>
          <p v-if="event.message">{{ event.message }}</p>
          <pre v-if="event.metadata">{{ JSON.stringify(event.metadata, null, 2) }}</pre>
          <p class="muted">Event {{ event.id }}</p>
        </div>
      </details>
    </div>
    <EmptyState v-else :icon="ClipboardCheck" :title="scope === 'manager' && !context ? 'Choose a Manager context' : 'No matching events'" />
    <QueryPager v-if="page" :busy="pending" :previous="cursors.length > 1" :next="page.next_cursor !== null" @previous="previous" @next="next" />
  </template>
</template>

<style scoped>
.history-filters { display: flex; flex-wrap: wrap; gap: 6px; }
.history-filters input { max-width: 150px; }
summary { cursor: pointer; flex-wrap: wrap; }
.event-detail { padding: 0 15px 8px; overflow-wrap: anywhere; }
.event-detail pre { white-space: pre-wrap; }
</style>
