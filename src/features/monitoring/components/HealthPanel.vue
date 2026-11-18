<script setup lang="ts">
import { computed } from 'vue'
import { ArrowUpRight } from 'lucide-vue-next'
import { formatDate } from '../../../common/format'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import { formatLatency, healthIsStale, latestHealth, type EndpointReading } from '../model/dashboard'
import { healthLabel } from '../model/monitoring'

const props = defineProps<{ endpoints: EndpointReading[]; timeline?: boolean }>()
defineEmits<{ open: [connection: string] }>()

const rows = computed(() => props.endpoints.map(entry => ({ ...entry, health: latestHealth(entry.view) })))
const incidents = computed(() => props.endpoints.flatMap(({ profile, view }) => (view?.incidents ?? [])
  .map(incident => ({ profile, incident }))).sort((left, right) => Date.parse(right.incident.openedAt) - Date.parse(left.incident.openedAt)))
const checks = computed(() => props.endpoints[0]?.view?.health ?? [])
</script>

<template>
  <WorkspacePanel title="Endpoint health" description="Status from stored checks">
    <div v-if="rows.length" class="monitoring-health-list">
      <div v-for="{ profile, view, health, failure } in rows" :key="profile.id" class="monitoring-health-row">
        <div class="monitoring-endpoint-name">
          <button v-if="!timeline" class="subtle" @click="$emit('open', profile.id)">{{ profile.name }} <ArrowUpRight :size="13" /></button>
          <strong v-else>{{ profile.name }}</strong>
          <small v-if="failure">{{ failure }}</small>
          <small v-else-if="health">{{ health.mode === 'signed' ? 'Signed S3' : 'HTTP' }} · {{ formatDate(health.checkedAt, true) }}</small>
          <small v-else>No check recorded</small>
        </div>
        <div><span class="monitoring-status" :data-state="health?.status">{{ health ? healthLabel[health.status] : failure ? 'Unavailable' : 'Unchecked' }}</span>
          <small v-if="healthIsStale(health, view?.retrievedAt)">Stale check</small></div>
        <div class="monitoring-latency"><strong>{{ formatLatency(health?.latencyMs) }}</strong>
          <small v-if="health?.baselineMs != null">{{ formatLatency(health.baselineMs) }} baseline</small>
        </div>
      </div>
    </div>
    <p v-else class="workspace-panel-message">Add a connection to check endpoint health.</p>
  </WorkspacePanel>

  <WorkspacePanel title="Recent incidents" description="Slow or down checks and their recovery">
    <ul v-if="incidents.length" class="monitoring-list">
      <li v-for="{ profile, incident } in incidents.slice(0, timeline ? 30 : 5)" :key="`${profile.id}:${incident.mode}:${incident.openedAt}`">
        <div><strong>{{ profile.name }}</strong><small>{{ incident.mode === 'signed' ? 'Signed S3' : 'HTTP' }} · {{ formatDate(incident.openedAt, true) }}</small></div>
        <div><span class="monitoring-status" :data-state="incident.recoveredAt ? 'up' : incident.severity">
          {{ incident.recoveredAt ? 'Recovered' : healthLabel[incident.severity] }}
        </span><small v-if="incident.recoveredAt">{{ formatDate(incident.recoveredAt, true) }}</small></div>
      </li>
    </ul>
    <p v-else class="workspace-panel-message">No incidents recorded.</p>
  </WorkspacePanel>

  <WorkspacePanel v-if="timeline" title="Check timeline" :description="`${checks.length} stored checks`">
    <div v-if="checks.length" class="workspace-table monitoring-table">
      <table>
        <thead><tr><th scope="col">Checked</th><th scope="col">Mode</th><th scope="col">Result</th><th scope="col">Latency</th><th scope="col">Baseline</th><th scope="col">HTTP status</th></tr></thead>
        <tbody>
          <tr v-for="check in checks" :key="`${check.mode}:${check.checkedAt}`">
            <th scope="row">{{ formatDate(check.checkedAt, true) }}</th><td>{{ check.mode === 'signed' ? 'Signed S3' : 'HTTP' }}</td>
            <td><span class="monitoring-status" :data-state="check.status">{{ healthLabel[check.status] }}</span><small v-if="check.detail">{{ check.detail }}</small></td>
            <td>{{ formatLatency(check.latencyMs) }}</td><td>{{ formatLatency(check.baselineMs) }}</td><td>{{ check.httpStatus ?? '—' }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-else class="workspace-panel-message">Use Check now to record an endpoint result.</p>
  </WorkspacePanel>
</template>
