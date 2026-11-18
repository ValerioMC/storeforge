<script setup lang="ts">
import { computed } from 'vue'
import { ArrowUpRight } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { AuditEvent } from '../../audit/model/audit'
import { identityAvailable, identityNames, serviceIdentities, type Connection } from '../../connection/model/connection'
import { workspaceAvailable } from '../../connection/model/workspace'
import { formatCount, supervisedTotals, type EndpointReading } from '../model/dashboard'
import type { MonitoringView } from '../model/monitoring'
import HealthPanel from './HealthPanel.vue'
import InfrastructureMap from './InfrastructureMap.vue'

const props = defineProps<{
  profiles: Connection[]; endpoints: EndpointReading[]; views: Record<string, MonitoringView>
  activity: AuditEvent[]; activityError: string
}>()
defineEmits<{ endpoint: [id: string]; connections: []; activity: []; metrics: [] }>()

const totals = computed(() => supervisedTotals(props.profiles, props.views))
const features = computed(() => serviceIdentities.map(identity => ({
  label: identityNames[identity],
  count: props.profiles.filter(profile => identityAvailable(profile, identity)).length,
})))
const enabledWorkspaces = computed(() => [
  { label: 'Browser', count: props.profiles.filter(profile => workspaceAvailable(profile, 'browser')).length },
  { label: 'Manager', count: props.profiles.filter(profile => workspaceAvailable(profile, 'manager')).length },
  { label: 'Ceph Admin', count: props.profiles.filter(profile => workspaceAvailable(profile, 'cephAdmin')).length },
  { label: 'Ceph traffic', count: features.value.find(feature => feature.label === identityNames.supervision)?.count ?? 0 },
])
const administration = computed(() => [
  { label: 'Saved connections', value: props.profiles.length },
  { label: 'Storage endpoints', value: props.endpoints.length },
  { label: 'Ceph gateways', value: props.endpoints.filter(({ profile }) => profile.provider === 'ceph').length },
  { label: 'Verified service identities', value: features.value.reduce((total, feature) => total + feature.count, 0) },
])
</script>

<template>
  <div class="monitoring-grid monitoring-admin-health"><HealthPanel :endpoints="endpoints" @open="$emit('endpoint', $event)" /></div>

  <WorkspacePanel title="Supervised storage and traffic" description="Ceph readings from saved connections">
    <template #actions><button class="subtle" :disabled="!profiles.length" @click="$emit('metrics')">Endpoint metrics <ArrowUpRight :size="14" /></button></template>
    <dl class="monitoring-facts">
      <div><dt>Stored bytes</dt><dd>{{ formatSize(totals.bytes) }}</dd></div>
      <div><dt>Objects</dt><dd>{{ formatCount(totals.objects) }}</dd></div>
      <div><dt>Requests · 24h</dt><dd>{{ formatCount(totals.requests) }}</dd></div>
      <div><dt>Transfer · 24h</dt><dd>{{ formatSize(totals.transfer) }}</dd></div>
    </dl>
    <p class="monitoring-panel-note">
      {{ totals.bucketsMeasured }} of {{ totals.bucketsVisible }} visible buckets measured<span v-if="totals.storagePartial"> · Partial storage coverage</span><span v-if="totals.storageStale"> · Includes stale storage samples</span>.
      {{ totals.trafficEndpoints }} of {{ totals.expectedEndpoints }} Ceph gateways reported traffic<span v-if="totals.trafficInterval">
        from {{ formatDate(totals.trafficInterval.start, true) }} to {{ formatDate(totals.trafficInterval.end, true) }}</span><span v-if="totals.trafficStale"> · Includes stale traffic samples</span>.
    </p>
  </WorkspacePanel>

  <WorkspacePanel title="Administration">
    <template #actions><button class="subtle" @click="$emit('connections')">Connections <ArrowUpRight :size="14" /></button></template>
    <dl class="monitoring-facts">
      <div v-for="item in administration" :key="item.label"><dt>{{ item.label }}</dt><dd>{{ formatCount(item.value) }}</dd></div>
    </dl>
  </WorkspacePanel>

  <div class="monitoring-grid">
    <WorkspacePanel title="Recent activity" description="Operations recorded on this device">
      <template #actions><button class="subtle" @click="$emit('activity')">View activity <ArrowUpRight :size="14" /></button></template>
      <Notice v-if="activityError" tone="error">{{ activityError }}</Notice>
      <ul v-else-if="activity.length" class="monitoring-list">
        <li v-for="event in activity.slice(0, 5)" :key="event.id">
          <div><strong>{{ event.action }}</strong><small v-if="event.resource">{{ event.resource }}</small></div>
          <div><span class="monitoring-status" :data-state="event.success ? 'up' : 'down'">{{ event.success ? 'Succeeded' : 'Failed' }}</span>
            <small>{{ formatDate(event.timestamp, true) }}</small></div>
        </li>
      </ul>
      <p v-else class="workspace-panel-message">No activity recorded.</p>
    </WorkspacePanel>
    <InfrastructureMap :profiles="profiles" :endpoints="endpoints" @open="$emit('endpoint', $event)" />
  </div>

  <WorkspacePanel title="Enabled features" description="Number of eligible profiles per feature">
    <dl class="monitoring-facts">
      <div v-for="feature in enabledWorkspaces" :key="feature.label"><dt>{{ feature.label }}</dt><dd>{{ formatCount(feature.count) }}</dd></div>
    </dl>
  </WorkspacePanel>
</template>
