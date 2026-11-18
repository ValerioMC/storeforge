<script setup lang="ts">
import { computed, nextTick, onActivated, ref, watch } from 'vue'
import { Activity, ArrowUpRight, Database, FolderCog, RefreshCw, Users, Wallet } from 'lucide-vue-next'
import { formatDate } from '../../../common/format'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { identityAvailable, providerNames, type Connection, type IdentityKind } from '../../connection/model/connection'
import { workspaceAvailable, type CephGroup, type MetricPage, type WorkspaceDestination } from '../../connection/model/workspace'
import { useAdminMonitoring } from '../composables/useAdminMonitoring'
import { useMonitoring } from '../composables/useMonitoring'
import { demoMonitoring } from '../model/demo'
import type { CheckMode } from '../model/monitoring'
import AdminOverview from './AdminOverview.vue'
import HealthPanel from './HealthPanel.vue'
import ManagerOverview from './ManagerOverview.vue'
import MonitoringMetrics from './MonitoringMetrics.vue'
import '../monitoring.css'

const props = defineProps<{
  workspace: 'manager' | 'cephAdmin' | 'admin'; pane: 'overview' | 'metrics' | 'health'; metric?: MetricPage
  profile?: Connection; profiles: Connection[]; compact: boolean
}>()
const emit = defineEmits<{ open: [destination: WorkspaceDestination, connection?: string]; create: [] }>()

const root = ref<HTMLElement>()
let returnFocus: HTMLElement | undefined
const allEndpoints = computed(() => props.workspace === 'admin' && props.pane === 'overview')
const target = computed(() => allEndpoints.value ? undefined : props.profile ?? props.profiles[0])
const demo = computed(() => props.profile?.id === 'demo')
const aggregateProfiles = computed(() => allEndpoints.value && !demo.value ? props.profiles : undefined)
const { view, pending, error, run } = useMonitoring(target)
const admin = useAdminMonitoring(aggregateProfiles)
const sample = computed(() => demo.value ? demoMonitoring : view.value)
const effectiveProfiles = computed(() => demo.value && props.profile ? [props.profile] : props.profiles)
const endpointRows = computed(() => target.value ? [{ profile: target.value, view: sample.value }] : [])
const overviewEndpoints = computed(() => demo.value && props.profile ? [{ profile: props.profile, view: demoMonitoring }] : admin.endpoints.value)
const adminViews = computed(() => demo.value ? { demo: demoMonitoring } : admin.views.value)
const title = computed(() => {
  if (props.pane === 'metrics') return 'Usage & metrics'
  if (props.pane === 'health') return 'Endpoint health'
  if (props.workspace === 'cephAdmin') return 'Ceph overview'
  return props.workspace === 'admin' ? 'Admin overview' : 'Storage overview'
})
const description = computed(() => {
  if (allEndpoints.value) return 'Health, storage and activity across saved connections'
  const profile = target.value
  return profile ? [profile.name, providerNames[profile.provider], profile.settings?.tenant, profile.settings?.account, profile.region].filter(Boolean).join(' · ') : ''
})
const checkMode = ref<CheckMode>('signed')
const checkModes = computed(() => [
  ...(target.value?.endpoint ? [{ value: 'http' as const, label: 'HTTP' }] : []),
  { value: 'signed' as const, label: 'Signed S3' },
])
const metricTab = computed({
  get: () => props.metric ?? 'usage',
  set: (metric: MetricPage) => openMetric(metric),
})
const options = computed(() => props.profiles.map(profile => ({ value: profile.id, label: profile.name })))
const selectedEndpoint = computed({
  get: () => target.value?.id ?? '',
  set: (id: string) => open({ workspace: 'admin', page: props.pane, metric: props.metric }, id),
})
const cephResources: { label: string; group: CephGroup; icon: typeof Database; identity: IdentityKind }[] = [
  { label: 'RGW accounts', group: 'Ceph accounts', icon: FolderCog, identity: 'adminOps' },
  { label: 'RGW users', group: 'Ceph users', icon: Users, identity: 'adminOps' },
  { label: 'RGW buckets', group: 'Ceph buckets', icon: Database, identity: 'cephAdmin' },
]
const shortcuts = computed(() => cephResources.filter(entry => demo.value || identityAvailable(target.value, entry.identity)))

watch(() => target.value?.endpoint, endpoint => { checkMode.value = endpoint ? 'http' : 'signed' }, { immediate: true })

onActivated(() => {
  if (allEndpoints.value) {
    if (!admin.pending.value && !demo.value) void admin.refresh()
  } else void run('cached')
  void nextTick(() => {
    if (returnFocus?.isConnected) returnFocus.focus()
    returnFocus = undefined
  })
})

function open(destination: WorkspaceDestination, connection = target.value?.id): void {
  const focused = document.activeElement
  if (focused instanceof HTMLElement && root.value?.contains(focused)) returnFocus = focused
  emit('open', destination, connection)
}

function openMetric(metric: MetricPage): void {
  open({ workspace: props.workspace, page: 'metrics', metric })
}

function openHealth(connection: string): void {
  open(props.workspace === 'cephAdmin' ? { workspace: 'cephAdmin', page: 'health' } : { workspace: 'admin', page: 'health' }, connection)
}

function openBucket(bucket: string): void {
  if (workspaceAvailable(target.value, 'manager')) open({ workspace: 'manager', page: 'buckets', bucket, filterBucket: true })
  else open({ workspace: 'cephAdmin', page: 'resources', group: 'Ceph buckets', bucket })
}

function refresh(): void {
  if (allEndpoints.value) void admin.refresh()
  else void run('cached')
}
</script>

<template>
  <section ref="root" class="workspace-page monitoring-workspace" :class="{ 'workspace-compact': compact }">
    <WorkspaceHeader :title="title" :description="description">
      <template #actions>
        <AppSelect v-if="workspace === 'admin' && pane !== 'overview' && !demo && options.length > 1"
          v-model="selectedEndpoint" :options="options" aria-label="Monitoring connection" />
        <button v-if="(allEndpoints && profiles.length || target) && !demo" :disabled="!!pending || admin.pending.value" @click="refresh">
          <RefreshCw :size="15" :class="{ spinning: pending === 'cached' || admin.pending.value }" /> Refresh cached
        </button>
        <button v-if="target && pane !== 'health' && !demo" class="primary" :disabled="!!pending" @click="run('collect')">
          <Database :size="15" /> {{ pending === 'collect' ? 'Collecting usage…' : 'Collect usage' }}
        </button>
        <template v-if="target && pane === 'health' && !demo">
          <AppSelect v-model="checkMode" :options="checkModes" aria-label="Endpoint check mode" :disabled="!!pending" />
          <button class="primary" :disabled="!!pending" @click="run('check', checkMode)"><Activity :size="15" /> {{ pending === 'check' ? 'Checking…' : 'Check now' }}</button>
        </template>
      </template>
    </WorkspaceHeader>

    <div v-if="!allEndpoints && target" class="monitoring-toolbar">
      <p role="status">
        <template v-if="pending === 'cached' && !sample">Reading stored samples…</template>
        <template v-else>
          <span v-if="sample?.usage">Collected {{ formatDate(sample.usage.collectedAt, true) }}<strong v-if="sample.stale"> · Stale data</strong></span>
          <span v-else-if="pane !== 'health'">No usage sample</span>
          <span v-if="sample">Retrieved {{ formatDate(sample.retrievedAt, true) }}</span>
        </template>
      </p>
      <div v-if="workspace === 'manager' && pane === 'overview'" class="workspace-actions">
        <button class="subtle" @click="openMetric('cost')"><Wallet :size="15" /> Cost inputs</button>
        <button :disabled="demo" @click="$emit('create')">New bucket</button>
      </div>
    </div>
    <Notice v-if="error" tone="error">{{ error }}</Notice>

    <div v-if="allEndpoints" class="monitoring-stack">
      <AdminOverview :profiles="effectiveProfiles" :endpoints="overviewEndpoints" :views="adminViews"
        :activity="demo ? [] : admin.activity.value" :activity-error="admin.activityError.value"
        @endpoint="openHealth" @metrics="openMetric('usage')" @connections="open({ workspace: 'admin', page: 'connections' })"
        @activity="open({ workspace: 'admin', page: 'activity', audit: true })" />
    </div>
    <p v-else-if="!target" class="monitoring-empty">Add a connection to view its metrics and health.</p>
    <div v-else-if="pane === 'metrics'" class="monitoring-stack">
      <MonitoringMetrics v-model="metricTab" :view="sample" :endpoint-traffic="workspace !== 'manager'" @bucket="openBucket" />
    </div>
    <div v-else-if="pane === 'health'" class="monitoring-stack">
      <HealthPanel :endpoints="endpointRows" timeline />
    </div>
    <div v-else-if="workspace === 'manager'" class="monitoring-stack">
      <ManagerOverview :profile="target" :view="sample" @metrics="openMetric" @inventory="open({ workspace: 'manager', page: 'buckets' })"
        @bucket="openBucket" @health="openHealth(target.id)" />
    </div>
    <div v-else class="monitoring-stack">
      <div class="monitoring-grid monitoring-admin-health"><HealthPanel :endpoints="endpointRows" @open="openHealth" /></div>
      <nav class="monitoring-shortcuts" aria-label="Ceph resources">
        <button @click="openMetric('usage')"><Activity :size="17" /> Usage & metrics <ArrowUpRight :size="14" /></button>
        <button v-for="item in shortcuts" :key="item.group" @click="open({ workspace: 'cephAdmin', page: 'resources', group: item.group })">
          <component :is="item.icon" :size="17" /> {{ item.label }} <ArrowUpRight :size="14" />
        </button>
      </nav>
    </div>
  </section>
</template>
