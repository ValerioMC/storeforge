<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RefreshCw } from 'lucide-vue-next'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { formatDate } from '../../../common/format'
import { workspaceSettingsApi } from '../../administration/api/workspace-settings'
import { operationsApi } from '../api/operational-events'
import { eventLabel, type BucketUsageSummary, type CostMonth, type EndpointHealth, type HistoryCollectionResult, type ServiceAuditEvent } from '../model/operational-events'

const props = defineProps<{ serviceId: string }>()
const endpoints = ref<{ value: string; label: string }[]>([])
const endpoint = ref('')
const pending = ref(false)
const running = ref(false)
const usagePending = ref(false)
const health = ref<EndpointHealth[]>()
const healthFailure = ref('')
const history = ref<string | null>()
const historyFailure = ref('')
const costs = ref<CostMonth>()
const costsFailure = ref('')
const expiry = ref<ServiceAuditEvent>()
const expiryFailure = ref('')
const usage = ref<BucketUsageSummary>()
const usageFailure = ref('')
const report = ref<HistoryCollectionResult>()
const runError = ref('')
const lastHealthCheck = computed(() => health.value?.map(item => item.checked_at).filter((value): value is string => !!value).sort().at(-1))
let usageRequest = 0

onMounted(refresh)

async function refresh(): Promise<void> {
  pending.value = true
  await Promise.all([loadHealth(), loadHistory(), loadCosts(), loadExpiry(), loadEndpoints()])
  pending.value = false
}

async function loadHealth(): Promise<void> {
  healthFailure.value = ''
  try {
    health.value = (await operationsApi.health(props.serviceId)).endpoints
  } catch (failure) {
    healthFailure.value = errorMessage(failure)
  }
}

async function loadHistory(): Promise<void> {
  historyFailure.value = ''
  try {
    history.value = (await operationsApi.historySummary(props.serviceId)).summary.latest_collected_at
  } catch (failure) {
    historyFailure.value = errorMessage(failure)
  }
}

async function loadCosts(): Promise<void> {
  costsFailure.value = ''
  try {
    const result = await operationsApi.cost(props.serviceId)
    costs.value = result.history.filter(month => month.coverage.days_collected > 0).sort((a, b) => a.month.localeCompare(b.month)).at(-1)
  } catch (failure) {
    costsFailure.value = errorMessage(failure)
  }
}

async function loadExpiry(): Promise<void> {
  expiryFailure.value = ''
  try {
    const page = await operationsApi.history(props.serviceId, 'admin', { search: 'enforce_access_key_expiration' })
    expiry.value = page.logs.find(item => item.action === 'enforce_access_key_expiration')
  } catch (failure) {
    expiryFailure.value = errorMessage(failure)
  }
}

async function loadEndpoints(): Promise<void> {
  try {
    endpoints.value = (await workspaceSettingsApi.endpoints(props.serviceId)).filter(item => item.provider === 'ceph')
      .map(item => ({ value: String(item.id), label: item.name }))
    const selected = endpoints.value.find(item => item.value === endpoint.value) ?? endpoints.value[0]
    if (selected) await loadUsage(selected.value)
  } catch (failure) {
    usageFailure.value = errorMessage(failure)
  }
}

async function loadUsage(id: string): Promise<void> {
  const request = ++usageRequest
  endpoint.value = id
  usagePending.value = true
  usageFailure.value = ''
  usage.value = undefined
  try {
    const result = await operationsApi.usage(props.serviceId, Number(id))
    if (request === usageRequest) usage.value = result.aggregate
  } catch (failure) {
    if (request === usageRequest) usageFailure.value = errorMessage(failure)
  } finally {
    if (request === usageRequest) usagePending.value = false
  }
}

async function collect(): Promise<void> {
  running.value = true
  runError.value = ''
  report.value = undefined
  try {
    report.value = await operationsApi.collectHistory(props.serviceId)
    await loadHistory()
  } catch (failure) {
    runError.value = errorMessage(failure)
  } finally {
    running.value = false
  }
}
</script>

<template>
  <div class="settings-toolbar collection-actions">
    <button type="button" :disabled="pending || running" @click="refresh"><RefreshCw :size="15" /> Refresh collections</button>
  </div>
  <p v-if="pending" role="status">Reading service collection records…</p>
  <section class="settings-sheet">
    <div class="settings-row">
      <div class="settings-description"><h2>Collection evidence</h2><p>The shared service scheduler determines next runs.</p></div>
      <span class="chip">Service owned</span>
    </div>
    <details>
      <summary class="settings-row"><strong>Endpoint health</strong><span>{{ lastHealthCheck ? formatDate(lastHealthCheck, true) : 'No check recorded' }}</span></summary>
      <Notice v-if="healthFailure" tone="error">{{ healthFailure }}</Notice>
      <div v-for="item in health" :key="item.name" class="settings-row">
        <div class="settings-description"><strong>{{ item.name }}</strong><p>{{ item.error_message }}</p></div>
        <span class="chip">{{ eventLabel(item.status) }}</span>
        <small v-if="item.checked_at">{{ formatDate(item.checked_at, true) }}</small>
      </div>
    </details>
    <div class="settings-row">
      <div class="settings-description"><strong>Usage and quota history</strong><p>{{ history ? formatDate(history, true) : 'No sample recorded' }}</p></div>
      <button type="button" :disabled="pending || running" @click="collect">Collect history now</button>
    </div>
    <Notice v-if="historyFailure || runError" tone="error">{{ runError || historyFailure }}</Notice>
    <div v-if="report" class="settings-row">
      <div class="settings-description">
        <strong>{{ report.status === 'skipped' ? 'Collection skipped' : `${report.subjects_processed} subjects processed` }}</strong>
        <p v-if="report.reason">{{ report.reason }}</p>
        <ul v-if="report.errors.length || report.warnings.length">
          <li v-for="(item, index) in report.errors" :key="`error-${index}`">{{ item.subject_type }} {{ item.subject_id }} · {{ item.error }}</li>
          <li v-for="(item, index) in report.warnings" :key="`warning-${index}`">{{ typeof item === 'string' ? item : `${item.subject_type} ${item.subject_id} · ${item.warning}` }}</li>
        </ul>
      </div>
    </div>
    <div class="settings-row">
      <div class="settings-description">
        <strong>Bucket usage</strong>
        <p>{{ usage?.newest_snapshot_at ? formatDate(usage.newest_snapshot_at, true) : 'No snapshot recorded' }}</p>
        <p v-if="usage">{{ usage.missing_bucket_count }} missing · {{ usage.partial_scan_count }} partial</p>
        <p v-for="warning in usage?.warnings" :key="warning">{{ warning }}</p>
      </div>
      <AppSelect :model-value="endpoint" :options="endpoints" aria-label="Bucket usage endpoint" placeholder="Choose a Ceph endpoint"
        :disabled="pending || usagePending" @update:model-value="loadUsage" />
    </div>
    <Notice v-if="usageFailure" tone="error">{{ usageFailure }}</Notice>
    <div class="settings-row">
      <div class="settings-description">
        <strong>Cost collection</strong><p>{{ costs ? `${costs.month} · ${costs.coverage.days_collected} of ${costs.coverage.days_in_month} days covered` : 'No covered month reported' }}</p>
      </div>
    </div>
    <Notice v-if="costsFailure" tone="error">{{ costsFailure }}</Notice>
    <div class="settings-row">
      <div class="settings-description">
        <strong>Scheduled key expiry</strong>
        <p>{{ expiry ? `Last recorded action ${formatDate(expiry.created_at, true)} · ${eventLabel(expiry.status)}` : 'No expiry action recorded' }}</p>
        <p v-if="expiry?.message">{{ expiry.message }}</p>
      </div>
    </div>
    <Notice v-if="expiryFailure" tone="error">{{ expiryFailure }}</Notice>
    <div class="settings-row">
      <div class="settings-description"><strong>Quota threshold checks</strong><p>Runs use the service scheduler. View published threshold events in the inbox.</p></div>
    </div>
  </section>
</template>

<style scoped>
h2 { font-size: 15px; margin: 0; }
.settings-row, .collection-actions { flex-wrap: wrap; }
summary { cursor: pointer; }
</style>
