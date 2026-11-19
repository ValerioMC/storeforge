<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ArrowUpRight, Save } from 'lucide-vue-next'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import { costComponents, zeroRates, type CostEndpoint, type CostSettings } from '../model/storage-costs'

const props = defineProps<{ settings: CostSettings; endpoints: CostEndpoint[]; busy: boolean }>()
const emit = defineEmits<{ save: [settings: CostSettings]; collection: [] }>()
const draft = ref<CostSettings>(copySettings(props.settings))
const target = ref('')
const options = computed(() => [
  { value: '', label: 'Default rates' },
  ...props.endpoints.map(endpoint => ({ value: endpoint.id, label: endpoint.name })),
])
const rate = computed(() => target.value ? draft.value.overrides[target.value] : draft.value.rates)
const dirty = computed(() => JSON.stringify(draft.value) !== JSON.stringify(props.settings))

watch(() => props.settings, settings => { draft.value = copySettings(settings) })
watch(options, values => { if (!values.some(option => option.value === target.value)) target.value = '' })

function copySettings(settings: CostSettings): CostSettings {
  return {
    ...settings,
    rates: settings.rates && { ...settings.rates },
    overrides: Object.fromEntries(Object.entries(settings.overrides).map(([id, value]) => [id, { ...value }])),
  }
}

function toggleRates(): void {
  if (target.value) {
    if (rate.value) delete draft.value.overrides[target.value]
    else draft.value.overrides[target.value] = { ...(draft.value.rates ?? zeroRates) }
  } else {
    draft.value.rates = rate.value ? null : { ...zeroRates }
  }
}
</script>

<template>
  <form class="cost-stack" @submit.prevent="emit('save', copySettings(draft))">
    <WorkspacePanel title="Pricing" description="Current rates apply to every month in this report.">
      <div class="cost-settings-body">
        <div class="cost-controls">
          <label>Currency <input v-model.trim="draft.currency" class="cost-currency" :disabled="busy" required maxlength="3" pattern="[A-Za-z]{3}" /></label>
          <label>Rate scope <AppSelect v-model="target" :options="options" :disabled="busy" aria-label="Rate scope" /></label>
          <button type="button" :disabled="busy" @click="toggleRates">{{ rate ? 'Remove rates' : target ? 'Add override' : 'Set default rates' }}</button>
        </div>
        <div v-if="rate" class="cost-rate-fields">
          <label v-for="component in costComponents" :key="component.key">
            {{ component.label }}
            <input v-model.number="rate[component.key]" type="number" :disabled="busy" required min="0" max="999999.999999" step="0.000001" />
            <small>{{ component.unit }}</small>
          </label>
        </div>
        <p v-else class="cost-help">{{ target ? 'This endpoint uses the default rates.' : 'Set default rates or add endpoint overrides to calculate estimates.' }}</p>
      </div>
    </WorkspacePanel>
    <WorkspacePanel title="Daily collection" description="Manual collection runs locally for managed RGW accounts and users.">
      <template #actions><button type="button" @click="emit('collection')">Open collection <ArrowUpRight :size="14" /></button></template>
      <div class="cost-settings-body cost-checks">
        <label><input v-model="draft.collectStorage" type="checkbox" :disabled="busy" /> Storage snapshots on the current UTC day</label>
        <label><input v-model="draft.collectTraffic" type="checkbox" :disabled="busy" /> RGW usage logs for the selected UTC day</label>
      </div>
    </WorkspacePanel>
    <div class="workspace-actions">
      <button type="submit" class="primary" :disabled="!dirty || busy"><Save :size="15" /> {{ busy ? 'Saving…' : 'Save settings' }}</button>
      <button type="button" :disabled="!dirty || busy" @click="draft = copySettings(settings)">Reset draft</button>
    </div>
  </form>
</template>
