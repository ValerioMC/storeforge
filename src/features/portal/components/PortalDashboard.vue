<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { ArrowUpRight, Database, FolderOpen } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import { formatCount } from '../../monitoring/model/dashboard'
import Notice from '../../../components/ui/Notice.vue'
import { usePortalTask } from '../composables/usePortalTask'
import { portalApi } from '../api/portal'
import type { PortalActivity, PortalAlert, PortalHealth, PortalProject, PortalScope, PortalTrends, PortalUsage } from '../model/spaces'

const props = defineProps<{ scope: PortalScope; project: PortalProject }>()
const emit = defineEmits<{ open: [space: string]; spaces: [] }>()
const usage = usePortalTask<PortalUsage>()
const trends = usePortalTask<PortalTrends>()
const health = usePortalTask<PortalHealth>()
const alerts = usePortalTask<PortalAlert[]>()
const activity = usePortalTask<PortalActivity[]>()
const usageSummary = usage.data
const { data: history } = trends
const { data: endpoint } = health
const { data: alertRows } = alerts
const { data: activityRows } = activity
const top = computed(() => [...usageSummary.value?.storage_spaces ?? []]
  .filter(space => space.used_bytes !== null).sort((a, b) => (b.used_bytes ?? 0) - (a.used_bytes ?? 0)).slice(0, 6))
const max = computed(() => Math.max(1, ...history.value?.points.map(point => point.used_bytes) ?? []))
const displayHistory = computed(() => history.value?.points.slice(-31) ?? [])
const pending = computed(() => [usage, trends, health, alerts, activity].some(panel => panel.pending.value))

onMounted(() => {
  const scope = props.scope
  void usage.run(() => portalApi.usage(scope))
  void trends.run(() => portalApi.trends(scope))
  void health.run(() => portalApi.health(scope))
  void alerts.run(() => portalApi.alerts(scope))
  void activity.run(() => portalApi.activity(scope))
})
</script>

<template>
  <p v-if="pending" class="muted portal-note" role="status">Loading project metrics…</p>
  <div class="portal-grid">
    <section class="portal-panel">
      <div class="portal-between"><h2><Database :size="16" /> Storage and quota</h2><button type="button" @click="emit('spaces')">Storage spaces <ArrowUpRight :size="14" /></button></div>
      <Notice v-if="usage.error.value" tone="error">{{ usage.error.value }}</Notice>
      <dl class="portal-statistics">
        <div><dt>Storage used</dt><dd>{{ formatSize(usageSummary?.used_bytes) }}</dd></div>
        <div><dt>Storage quota</dt><dd>{{ formatSize(usageSummary?.quota_max_size_bytes) }}</dd></div>
        <div><dt>Objects</dt><dd>{{ formatCount(usageSummary?.used_objects) }}</dd></div>
        <div><dt>Object quota</dt><dd>{{ formatCount(usageSummary?.quota_max_objects) }}</dd></div>
      </dl>
      <Notice v-if="trends.error.value" tone="error">{{ trends.error.value }}</Notice>
      <template v-else-if="displayHistory.length">
        <h3 class="portal-trend-title">Storage history · {{ formatDate(displayHistory[0]?.period_start) }} to {{ formatDate(displayHistory.at(-1)?.period_start) }}</h3>
        <div class="portal-trend" aria-label="Storage usage and quota history">
          <div v-for="point in displayHistory" :key="point.period_start" class="portal-trend-day"
            :title="`${formatDate(point.period_start)} · ${formatSize(point.used_bytes)}${point.max_usage_ratio_pct === null ? '' : ` · ${point.max_usage_ratio_pct.toFixed(1)}% quota`}`">
            <span :style="{ height: `${point.used_bytes / max * 100}%` }" />
            <i v-if="point.max_usage_ratio_pct !== null" :style="{ bottom: `${Math.min(point.max_usage_ratio_pct, 100)}%` }" />
          </div>
        </div>
        <p class="muted portal-note">Bars show storage used. Markers show the share of quota used.</p>
      </template>
      <p v-else-if="history" class="muted portal-note">{{ history.unavailable_reason || 'No storage history has been collected.' }}</p>
    </section>
    <section class="portal-panel">
      <h2><FolderOpen :size="16" /> Top storage spaces</h2>
      <ul v-if="top.length" class="portal-list">
        <li v-for="space in top" :key="space.id" class="portal-between">
          <button type="button" class="subtle portal-button-label" @click="emit('open', space.id)">{{ space.name }}</button>
          <span>{{ formatSize(space.used_bytes) }}</span>
        </li>
      </ul>
      <p v-else-if="!pending" class="muted portal-note">No space usage measurements are available.</p>
    </section>
    <section class="portal-panel">
      <h2>Recent activity</h2>
      <Notice v-if="activity.error.value" tone="error">{{ activity.error.value }}</Notice>
      <ul v-else-if="activityRows?.length" class="portal-list">
        <li v-for="event in activityRows" :key="event.id" class="portal-between">
          <div>
            <button v-if="event.storage_space_id" type="button" class="subtle portal-button-label" @click="emit('open', event.storage_space_id)">{{ event.target || event.action }}</button>
            <strong v-else>{{ event.target || event.action }}</strong>
            <div class="muted">{{ event.action }} · {{ event.actor }}</div>
          </div>
          <span class="muted">{{ formatDate(event.created_at, true) }} · {{ event.status }}</span>
        </li>
      </ul>
      <p v-else-if="activityRows" class="muted portal-note">No recent project activity.</p>
    </section>
    <section class="portal-panel">
      <h2>Service status</h2>
      <Notice v-if="health.error.value" tone="error">{{ health.error.value }}</Notice>
      <ul v-else-if="endpoint?.endpoints.length" class="portal-list">
        <li v-for="entry in endpoint.endpoints" :key="entry.name" class="portal-between">
          <span>{{ entry.name }} <span class="muted">{{ formatDate(entry.checked_at, true) }}</span></span>
          <span class="chip">{{ entry.is_stale ? 'Stale' : entry.status }}</span>
        </li>
      </ul>
      <p v-else class="muted portal-note">{{ project.storage_endpoint_name }} · Status unavailable</p>
      <h2>Alerts</h2>
      <Notice v-if="alerts.error.value" tone="error">{{ alerts.error.value }}</Notice>
      <ul v-else-if="alertRows?.length" class="portal-list">
        <li v-for="alert in alertRows" :key="alert.id">
          <button v-if="alert.storage_space_id" type="button" class="subtle portal-button-label" @click="emit('open', alert.storage_space_id)">{{ alert.title }}</button>
          <strong v-else>{{ alert.title }}</strong>
          <p class="muted portal-note">{{ alert.severity_label }} · {{ alert.description }}</p>
        </li>
      </ul>
      <p v-else-if="alertRows" class="muted portal-note">No active project alerts.</p>
    </section>
  </div>
</template>

<style scoped>
h2 svg { vertical-align: -3px; margin-right: 3px; }
.portal-trend-title { color: var(--muted); font-size: 12px; }
.portal-trend { display: flex; gap: 3px; height: 110px; align-items: stretch; border-bottom: 1px solid var(--line); }
.portal-trend-day { flex: 1; position: relative; min-width: 2px; }
.portal-trend-day span { background: var(--accent); position: absolute; inset: auto 0 0; opacity: .65; border-radius: 2px 2px 0 0; }
.portal-trend-day i { position: absolute; display: block; height: 2px; inset-inline: 0; background: var(--ink); }
</style>
