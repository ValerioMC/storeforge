<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { workspaceSettingsApi } from '../api/workspace-settings'
import {
  pendingRotations, rotationAvailability, rotationKinds, rotationLabels, rotationRequiresDelete, settingLabel,
  type RotationEndpoint, type RotationKind, type RotationPlan, type RotationRecovery, type RotationReport,
} from '../model/workspace-settings'

const props = defineProps<{ serviceId: string }>()
const endpoints = ref<RotationEndpoint[]>([])
const selectedEndpoints = ref<number[]>([])
const selectedKinds = ref<RotationKind[]>([])
const retirement = ref<'disable' | 'delete'>('disable')
const recovery = ref<RotationRecovery>()
const review = ref<RotationPlan>()
const results = ref<RotationReport['results']>([])
const latest = ref<RotationReport['summary']>()
const pending = ref(false)
const error = ref('')
const journalError = ref('')
const ambiguous = ref(false)
const retirementOptions = [{ value: 'disable', label: 'Disable old keys' }, { value: 'delete', label: 'Delete old keys' }] as const
const requiresDelete = computed(() => selectedKinds.value.some(rotationRequiresDelete))
const availableIds = computed(() => endpoints.value.filter(endpoint => selectedKinds.value.length && selectedKinds.value.every(kind => !rotationAvailability(endpoint, kind))).map(endpoint => endpoint.id))
const selectedIds = computed(() => selectedEndpoints.value.filter(id => availableIds.value.includes(id)))
const recoveryEndpoint = computed(() => endpoints.value.find(endpoint => endpoint.id === recovery.value?.endpoint_id))

onMounted(refreshJournal)

async function refreshJournal(): Promise<void> {
  pending.value = true
  journalError.value = ''
  try {
    endpoints.value = await workspaceSettingsApi.endpoints(props.serviceId)
  } catch (failure) {
    journalError.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}

function reviewSelection(): void {
  review.value = {
    endpoint_ids: [...selectedIds.value], key_types: [...selectedKinds.value],
    deactivate_only: !requiresDelete.value && retirement.value === 'disable',
  }
}

function reviewRecovery(item: RotationRecovery): void {
  recovery.value = { ...item, deactivate_only: !rotationRequiresDelete(item.kind) && retirement.value === 'disable' }
}

async function execute(operation: () => Promise<RotationReport>): Promise<void> {
  pending.value = true
  review.value = undefined
  recovery.value = undefined
  error.value = ''
  ambiguous.value = false
  try {
    const report = await operation()
    latest.value = report.summary
    retainResults(report.results)
    selectedEndpoints.value = []
  } catch (failure) {
    error.value = errorMessage(failure)
    ambiguous.value = true
  } finally {
    await refreshJournal()
  }
}

function retainResults(items: RotationReport['results']): void {
  const byTarget = new Map(results.value.map(item => [resultKey(item), item]))
  for (const item of items) byTarget.set(resultKey(item), item)
  results.value = [...byTarget.values()]
}

function resultKey(item: RotationReport['results'][number]): string {
  return `${item.endpoint_id}:${item.key_type}:${item.target_id}`
}

function startRotation(): void {
  const plan = review.value
  if (plan) void execute(() => workspaceSettingsApi.rotate(props.serviceId, plan))
}

function resumeRotation(): void {
  const item = recovery.value
  if (item) void execute(() => workspaceSettingsApi.recover(props.serviceId, item))
}
</script>

<template>
  <div class="rotation">
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <Notice v-if="journalError" tone="error">{{ journalError }}</Notice>
    <Notice v-if="ambiguous">The outcome is unknown. Check the service journal and audit history before starting another rotation.</Notice>
    <section class="settings-sheet">
      <div class="settings-row"><h2>Key categories</h2></div>
      <div class="settings-row categories">
        <label v-for="kind in rotationKinds" :key="kind">
          <input v-model="selectedKinds" type="checkbox" :value="kind" :disabled="pending" />{{ rotationLabels[kind] }}
        </label>
      </div>
      <div class="settings-row">
        <label for="retire-keys">Previous keys</label>
        <AppSelect v-if="!requiresDelete" id="retire-keys" v-model="retirement" :options="retirementOptions" :disabled="pending" />
        <span v-else>Managed technical credentials require deletion after validation.</span>
      </div>
    </section>
    <section class="settings-sheet">
      <div class="settings-row">
        <h2>Endpoints and rotation journal</h2>
        <button type="button" :disabled="pending" @click="refreshJournal">Refresh journal</button>
      </div>
      <div v-for="endpoint in endpoints" :key="endpoint.id" class="settings-row endpoint">
        <label>
          <input v-model="selectedEndpoints" type="checkbox" :value="endpoint.id" :disabled="pending || !availableIds.includes(endpoint.id)" />
          {{ endpoint.name }}
        </label>
        <div v-if="selectedKinds.length" class="settings-description">
          <p v-for="kind in selectedKinds" :key="kind">{{ rotationLabels[kind] }} · {{ rotationAvailability(endpoint, kind) || 'Eligible' }}</p>
        </div>
        <div v-for="item in pendingRotations(endpoint)" :key="item.kind" class="recovery">
          <span class="chip">{{ rotationLabels[item.kind] }} · {{ settingLabel(item.phase) }}</span>
          <button type="button" :disabled="pending" @click="reviewRecovery(item)">Resume {{ rotationLabels[item.kind] }}</button>
        </div>
      </div>
      <p v-if="pending" class="settings-row" role="status">Waiting for the service…</p>
      <p v-else-if="!endpoints.length && !journalError" class="settings-row">No storage endpoints found.</p>
    </section>
    <div class="settings-toolbar">
      <button type="button" class="primary" :disabled="pending || ambiguous || !selectedIds.length || !selectedKinds.length" @click="reviewSelection">Review rotation</button>
    </div>
    <section v-if="latest" class="settings-sheet">
      <div class="settings-row">
        <h2>Rotation results</h2>
        <span>{{ latest.rotated }} rotated · {{ latest.failed }} failed · {{ latest.skipped }} skipped in the latest response</span>
      </div>
      <div v-for="item in results" :key="resultKey(item)" class="settings-row">
        <div class="settings-description">
          <strong>{{ item.endpoint_name }} · {{ item.target_label || rotationLabels[item.key_type] }}</strong>
          <p>{{ item.message }}</p>
        </div>
        <span class="chip">{{ settingLabel(item.status) }}</span>
        <span v-if="item.rotation_phase" class="chip">{{ settingLabel(item.rotation_phase) }}</span>
      </div>
    </section>
    <ConfirmDialog v-if="review" title="Rotate selected credentials?"
      :description="review.deactivate_only ? 'The service will disable the previous keys after validating their replacements.' : 'The service will delete the previous keys after validating their replacements.'"
      action="Start rotation" danger @close="review = undefined" @confirm="startRotation">
      <ul>
        <li v-for="endpoint in endpoints.filter(item => review?.endpoint_ids.includes(item.id))" :key="endpoint.id">
          {{ endpoint.name }} · {{ review.key_types.filter(kind => !rotationAvailability(endpoint, kind)).map(kind => rotationLabels[kind]).join(', ') }}
        </li>
      </ul>
    </ConfirmDialog>
    <ConfirmDialog v-if="recovery" title="Resume pending rotation?"
      :description="`${recoveryEndpoint?.name || ''} · ${rotationLabels[recovery.kind]} · ${settingLabel(recovery.phase)}. The service revalidates the replacement before it ${recovery.deactivate_only ? 'disables' : 'deletes'} the previous key.`"
      action="Resume rotation" danger @close="recovery = undefined" @confirm="resumeRotation" />
  </div>
</template>

<style scoped>
.rotation { display: grid; gap: 10px; }
.settings-row { flex-wrap: wrap; }
h2 { font-size: 15px; margin: 0; }
.categories { justify-content: flex-start; gap: 8px 18px; }
.categories label, .endpoint label, .recovery { display: flex; align-items: center; gap: 6px; }
</style>
