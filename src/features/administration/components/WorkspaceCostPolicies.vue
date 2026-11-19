<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { workspaceSettingsApi } from '../api/workspace-settings'
import { serviceRateLabels, type RotationEndpoint, type ServiceCostRates, type ServiceCostSettings } from '../model/workspace-settings'

const props = defineProps<{ serviceId: string }>()
const settings = ref<ServiceCostSettings>()
const endpoints = ref<RotationEndpoint[]>([])
const pending = ref(false)
const error = ref('')
const saved = ref(false)
const addEndpoint = ref('')
const endpointOptions = computed(() => endpoints.value
  .filter(endpoint => !settings.value?.endpoint_overrides.some(item => item.storage_endpoint_id === endpoint.id))
  .map(endpoint => ({ value: String(endpoint.id), label: endpoint.name })))

onMounted(load)

function zeroRates(): ServiceCostRates {
  return { storage_gb_month_price: '0', egress_gb_price: '0', ingress_gb_price: '0', requests_per_1000_price: '0' }
}

function startModel(): void {
  if (settings.value) settings.value.model = { currency: '', ...zeroRates() }
}

function removeModel(): void {
  if (settings.value) settings.value = { model: null, endpoint_overrides: [] }
}

function addOverride(): void {
  const model = settings.value?.model
  if (!settings.value || !model || !addEndpoint.value) return
  const row = Object.fromEntries(Object.keys(serviceRateLabels).map(key => [key, model[key as keyof ServiceCostRates]])) as ServiceCostRates
  settings.value.endpoint_overrides.push({ ...row, storage_endpoint_id: Number(addEndpoint.value) })
  addEndpoint.value = ''
}

function endpointName(id: number): string {
  return endpoints.value.find(endpoint => endpoint.id === id)?.name ?? `Endpoint ${id}`
}

async function load(): Promise<void> {
  pending.value = true
  try {
    const [model, inventory] = await Promise.all([
      workspaceSettingsApi.costSettings(props.serviceId), workspaceSettingsApi.endpoints(props.serviceId),
    ])
    settings.value = model
    endpoints.value = inventory
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}

async function save(): Promise<void> {
  if (!settings.value || pending.value) return
  pending.value = true
  error.value = ''
  saved.value = false
  try {
    settings.value = await workspaceSettingsApi.costSettings(props.serviceId, settings.value)
    saved.value = true
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <p v-if="pending && !settings" role="status">Reading cost settings…</p>
  <form v-if="settings" class="cost-form" @submit.prevent="save" @input="saved = false">
    <Notice v-if="saved" tone="success">Cost settings saved.</Notice>
    <fieldset class="settings-sheet" :disabled="pending">
      <div class="settings-row">
        <h2>Global cost model</h2>
        <button v-if="!settings.model" type="button" @click="startModel">Configure rates</button>
        <button v-else type="button" @click="removeModel">Remove model and overrides</button>
      </div>
      <template v-if="settings.model">
        <label class="settings-row">Currency code <input v-model.trim="settings.model.currency" required pattern="[A-Za-z]{3}" maxlength="3" placeholder="EUR" /></label>
        <label v-for="(label, key) in serviceRateLabels" :key="key" class="settings-row">
          {{ label }} <input v-model.trim="settings.model[key]" type="text" inputmode="decimal" pattern="[0-9]+([.][0-9]{1,6})?" required />
        </label>
      </template>
    </fieldset>
    <fieldset v-if="settings.model" class="settings-sheet" :disabled="pending">
      <div class="settings-row">
        <h2>Endpoint rates</h2>
        <AppSelect v-model="addEndpoint" :options="endpointOptions" aria-label="Cost endpoint" placeholder="Select endpoint" :disabled="pending" />
        <button type="button" :disabled="!addEndpoint" @click="addOverride">Add override</button>
      </div>
      <details v-for="(override, index) in settings.endpoint_overrides" :key="override.storage_endpoint_id">
        <summary class="settings-row">{{ endpointName(override.storage_endpoint_id) }}</summary>
        <label v-for="(label, key) in serviceRateLabels" :key="key" class="settings-row">
          {{ label }} <input v-model.trim="override[key]" type="text" inputmode="decimal" pattern="[0-9]+([.][0-9]{1,6})?" required />
        </label>
        <div class="settings-row"><button type="button" @click="settings.endpoint_overrides.splice(index, 1)">Remove override</button></div>
      </details>
    </fieldset>
    <div class="settings-toolbar"><button type="submit" class="primary" :disabled="pending">Save cost settings</button></div>
  </form>
</template>

<style scoped>
.cost-form { display: grid; gap: 10px; }
fieldset { padding: 0; min-width: 0; }
h2 { font-size: 15px; margin: 0; }
.settings-row { flex-wrap: wrap; }
</style>
