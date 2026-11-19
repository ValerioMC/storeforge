<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatDate } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { readMultisiteFlow } from '../api/ceph-multisite'
import { availabilityLabels, zoneFlows, zoneMappings, type FlowDirection, type Observation, type Period, type SyncStatus, type Topology, type ZoneBinding } from '../model/ceph-multisite'
import CephSyncObservation from './CephSyncObservation.vue'

const props = defineProps<{ topology: Topology; period: Period; endpoints: { label: string; value: string; binding: ZoneBinding }[] }>()
const destination = ref(props.endpoints.find(item => item.value === props.topology.anchor.connection)?.value ?? props.endpoints[0]?.value ?? '')
const activeZone = computed(() => props.endpoints.find(item => item.value === destination.value))
const zone = computed(() => props.period.zones.find(zone => zone.id === activeZone.value?.binding.zone))
const flows = computed(() => zone.value ? zoneFlows(props.period, zone.value) : [])
const mappings = computed(() => zoneMappings(props.topology))
const unmapped = computed(() => props.topology.registrations.filter(({ local }) =>
  local.availability !== 'available' || !local.data || local.data.realmId !== props.period.realmId ||
  !props.period.zones.some(zone => zone.id === local.data?.id)))
const records = ref<Record<string, Observation<SyncStatus>>>({})
const picked = ref('')
const record = computed(() => records.value[JSON.stringify([destination.value, picked.value])])
const pickedFlow = computed(() => flows.value.find(flow => flow.key === picked.value))
const busy = ref(false)
const error = ref('')

function changeDestination(): void {
  error.value = ''
  picked.value = ''
}

async function query(flow: FlowDirection): Promise<void> {
  const endpoint = activeZone.value
  if (!endpoint || busy.value) return
  const key = JSON.stringify([endpoint.value, flow.key])
  picked.value = flow.key
  error.value = ''
  busy.value = true
  try {
    records.value[key] = await readMultisiteFlow({ destination: endpoint.binding, family: flow.family, sourceZone: flow.source.id })
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}

function flowResult(flow: FlowDirection): string {
  const result = records.value[JSON.stringify([destination.value, flow.key])]
  if (!result) return 'Awaiting check'
  return result.data ? `${result.data.phase} · ${result.data.shards.length}${result.data.expectedShards === null ? '' : ` / ${result.data.expectedShards}`} shards`
    : availabilityLabels[result.availability]
}
</script>

<template>
  <WorkspacePanel title="Realm and period">
    <div class="multisite-content">
      <dl class="multisite-facts">
        <div><dt>Realm</dt><dd>{{ topology.realm.data?.name || period.realmId }}<template v-if="!topology.realm.data"> · {{ availabilityLabels[topology.realm.availability] }}</template></dd></div>
        <div><dt>Period</dt><dd>{{ period.id }} · Epoch {{ period.epoch ?? 'Unavailable' }}</dd></div>
        <div><dt>Realm epoch</dt><dd>{{ period.realmEpoch ?? 'Unavailable' }}</dd></div>
        <div><dt>Collected</dt><dd>{{ formatDate(topology.collectedAt, true) }}</dd></div>
      </dl>
    </div>
  </WorkspacePanel>
  <WorkspacePanel title="Zones and registered gateways">
    <div class="multisite-table">
      <table class="report-table">
        <thead><tr><th scope="col">Zone</th><th scope="col">Zone group</th><th scope="col">Role</th><th scope="col">Saved gateways</th></tr></thead>
        <tbody>
          <tr v-for="{ zone: item, state, endpoints: matches } in mappings" :key="item.id">
            <th scope="row">{{ item.name }}<small class="muted">{{ item.id }}</small></th>
            <td>{{ item.groupName }}</td>
            <td>{{ item.id === period.masterZone ? 'Metadata master' : 'Replica' }}<small v-if="item.readOnly" class="muted">Read only</small></td>
            <td v-if="state === 'missing'">No registered match</td>
            <td v-else>{{ matches.map(match => match.name).join(', ') }}<small v-if="state === 'ambiguous'" class="muted">Ambiguous · select a gateway to diagnose</small></td>
          </tr>
          <tr v-for="{ scope, name, local } in unmapped" :key="scope.connection">
            <th scope="row">{{ local.data?.name || '—' }}</th><td>—</td><td>—</td>
            <td>{{ name }}<small class="muted">{{ local.data ? 'Outside this period' : availabilityLabels[local.availability] }}<template v-if="!local.data && local.status"> · HTTP {{ local.status }}</template></small></td>
          </tr>
        </tbody>
      </table>
    </div>
  </WorkspacePanel>
  <WorkspacePanel title="Sync coverage">
    <div class="multisite-content">
      <label class="multisite-field">Destination gateway
        <AppSelect v-model="destination" :options="endpoints" placeholder="Select a verified gateway" :disabled="busy" @update:model-value="changeDestination" />
      </label>
      <p v-if="!flows.length" class="muted">No metadata or data flows are declared for this destination.</p>
      <div v-else class="multisite-table">
        <table class="report-table">
          <thead><tr><th scope="col">Source zone</th><th scope="col">Family</th><th scope="col">Coverage</th><th scope="col">Action</th></tr></thead>
          <tbody>
            <tr v-for="flow in flows" :key="flow.key">
              <th scope="row">{{ flow.source.name }}</th><td>{{ flow.family === 'metadata' ? 'Metadata' : 'Data' }}</td>
              <td>{{ flowResult(flow) }}</td>
              <td><button type="button" :disabled="busy" @click="query(flow)">Read status</button></td>
            </tr>
          </tbody>
        </table>
      </div>
      <p v-if="busy" role="status">Reading sync coverage…</p>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <template v-if="pickedFlow && record && !busy">
        <h3>{{ pickedFlow.source.name }} → {{ pickedFlow.destination.name }} · {{ pickedFlow.family }}</h3>
        <CephSyncObservation :value="record" />
      </template>
    </div>
  </WorkspacePanel>
</template>

<style scoped>
.multisite-content { display: grid; gap: 12px; padding: 14px 16px; }
.multisite-facts { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(190px, 100%), 1fr)); gap: 12px 20px; margin: 0; }
.multisite-facts dt { color: var(--muted); font-size: 12px; }
.multisite-facts dd { margin: 4px 0 0; overflow-wrap: anywhere; }
.multisite-field { display: grid; gap: 5px; max-width: 25rem; }
.multisite-table { overflow: auto; }
.multisite-table small { display: block; }
.multisite-table td, .multisite-table th { overflow-wrap: anywhere; }
</style>
