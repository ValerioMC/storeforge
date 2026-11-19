<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ExternalLink, RefreshCw } from 'lucide-vue-next'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { formatDate } from '../../../common/format'
import { sharedServiceApi } from '../../connection/api/shared-service'
import type { SharedAction } from '../../connection/model/shared-service'
import { operationsApi } from '../api/operational-events'
import { eventLabel, type WebhookDelivery, type WebhookEndpoint } from '../model/operational-events'

const props = defineProps<{ serviceId: string; authorized: boolean }>()
const endpoints = ref<WebhookEndpoint[]>()
const deliveries = ref<WebhookDelivery[]>()
const selected = ref<WebhookEndpoint>()
const pending = ref(false)
const error = ref('')

onMounted(() => {
  if (props.authorized) void refresh()
})

async function refresh(): Promise<void> {
  await run(async () => {
    endpoints.value = await operationsApi.webhooks(props.serviceId)
    selected.value = endpoints.value.find(endpoint => endpoint.id === selected.value?.id)
    deliveries.value = selected.value ? await operationsApi.deliveries(props.serviceId, selected.value.id) : undefined
  })
}

async function showDeliveries(endpoint: WebhookEndpoint): Promise<void> {
  selected.value = endpoint
  deliveries.value = undefined
  await run(async () => {
    deliveries.value = await operationsApi.deliveries(props.serviceId, endpoint.id)
  })
}

async function open(action: SharedAction): Promise<void> {
  await run(() => sharedServiceApi.open(props.serviceId, action))
}

async function run(operation: () => Promise<void>): Promise<void> {
  pending.value = true
  error.value = ''
  try {
    await operation()
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <div class="settings-toolbar webhook-controls">
    <button v-if="authorized" type="button" :disabled="pending" @click="refresh"><RefreshCw :size="15" /> Refresh webhooks</button>
    <button type="button" :disabled="pending" @click="open('newWebhook')"><ExternalLink :size="15" /> New webhook</button>
  </div>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <Notice v-else-if="!authorized">Authorize a superadmin service token to view subscriptions and deliveries.</Notice>
  <p v-if="pending" role="status">Waiting for the service…</p>
  <section v-if="endpoints" class="settings-sheet">
    <p v-if="!endpoints.length" class="settings-row">No webhook subscriptions.</p>
    <div v-for="endpoint in endpoints" :key="endpoint.id" class="settings-row">
      <div class="settings-description">
        <strong>{{ endpoint.name }}</strong>
        <p>{{ endpoint.url }}</p>
        <p>{{ endpoint.event_types.includes('*') ? 'All events' : endpoint.event_types.join(', ') }}</p>
      </div>
      <span class="chip">{{ endpoint.enabled ? 'Enabled' : 'Disabled' }}</span>
      <span v-if="endpoint.last_delivery" class="chip">{{ eventLabel(endpoint.last_delivery.status) }}</span>
      <button type="button" :disabled="pending" @click="showDeliveries(endpoint)">Deliveries</button>
      <button type="button" :disabled="pending" @click="open({ editWebhook: { id: endpoint.id } })"><ExternalLink :size="15" /> Edit in browser</button>
    </div>
  </section>
  <section v-if="selected && deliveries" class="settings-sheet">
    <div class="settings-row"><h2>{{ selected.name }} deliveries</h2></div>
    <p v-if="!deliveries.length" class="settings-row">No deliveries recorded.</p>
    <details v-for="delivery in deliveries" :key="delivery.id">
      <summary class="settings-row">
        <div class="settings-description">
          <strong>{{ delivery.event_type }}</strong>
          <p>Delivery {{ delivery.id }} · {{ formatDate(delivery.updated_at, true) }}</p>
        </div>
        <span v-if="delivery.is_test" class="chip">Test event</span>
        <span class="chip">{{ eventLabel(delivery.status) }}</span>
        <span>{{ delivery.attempt_count }} {{ delivery.attempt_count === 1 ? 'attempt' : 'attempts' }}</span>
      </summary>
      <div class="delivery-detail">
        <p v-if="delivery.last_http_status">Last response HTTP {{ delivery.last_http_status }}</p>
        <p v-if="delivery.last_error">{{ delivery.last_error }}</p>
        <p v-if="delivery.delivered_at">Delivered {{ formatDate(delivery.delivered_at, true) }}</p>
      </div>
    </details>
  </section>
</template>

<style scoped>
h2 { font-size: 15px; margin: 0; }
.settings-row, .webhook-controls { flex-wrap: wrap; gap: 8px; }
.webhook-controls { display: flex; }
.settings-description, .settings-description p { overflow-wrap: anywhere; }
summary { cursor: pointer; }
.delivery-detail { padding: 0 15px 8px; }
</style>
