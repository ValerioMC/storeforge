<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ArrowUpRight, ChevronRight, Database, Plus, RefreshCw } from 'lucide-vue-next'
import { formatDate } from '../../../common/format'
import WorkspaceDrawer from '../../../components/shell/WorkspaceDrawer.vue'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import WorkspaceMetric from '../../../components/shell/WorkspaceMetric.vue'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import { useBrowser } from '../../browser/store/browser'
import BucketSettings from '../../bucket/components/BucketSettings.vue'
import { capabilityNames, identityCapability, identityNames, providerNames, serviceIdentities, type Connection } from '../model/connection'

const props = defineProps<{ profile: Connection; inventory: boolean; bucket?: string; browseAvailable: boolean; demo: boolean; compact: boolean }>()
const emit = defineEmits<{ inventory: []; inspect: [bucket: string]; browse: [bucket: string]; create: []; connections: []; tools: [bucket: string] }>()
const browser = useBrowser()
const filter = ref('')
const bucketTab = ref<'details' | 'configuration'>('details')
const bucketTabs = [{ value: 'details', label: 'Details' }, { value: 'configuration', label: 'Configuration' }] as const
const shownBuckets = computed(() => props.inventory ? browser.buckets.filter(bucket =>
  bucket.name.toLowerCase().includes(filter.value.trim().toLowerCase())) : browser.buckets.slice(0, 6))
const bucketCount = computed(() => browser.bucketLoading || browser.bucketError ? '—' : browser.buckets.length)
const checkRows = computed(() => {
  const applicable = props.profile.provider === 'ceph' ? serviceIdentities : ['runtime'] as const
  return (['s3', ...applicable] as const).map(identity => ({
    label: identityNames[identity],
    state: props.demo ? (identity === 's3' ? 'detected' : 'unavailable') : identityCapability(props.profile, identity),
  }))
})
const verifiedCount = computed(() => checkRows.value.filter(row => row.state === 'detected').length)
const openBucket = computed(() => browser.buckets.find(bucket => bucket.name === props.bucket))

watch(() => props.bucket, () => { bucketTab.value = 'details' })

async function bucketRemoved(): Promise<void> {
  emit('inventory')
  await browser.loadBuckets()
}
</script>

<template>
  <section class="workspace-page manager-workspace" :class="{ 'workspace-compact': compact }">
    <WorkspaceHeader :title="inventory ? 'Buckets' : 'Storage overview'"
      :description="`${providerNames[profile.provider]} · ${profile.region} · ${profile.endpoint || 'AWS regional endpoint'}`">
      <template #actions>
        <button :disabled="browser.bucketLoading" @click="browser.loadBuckets()"><RefreshCw :size="16" :class="{ spinning: browser.bucketLoading }" /> Refresh</button>
        <button class="primary" :disabled="demo" @click="emit('create')"><Plus :size="16" /> New bucket</button>
      </template>
    </WorkspaceHeader>

    <div v-if="!inventory" class="workspace-metrics">
      <WorkspaceMetric title="Visible buckets" :value="bucketCount" :detail="profile.knownBuckets.length ? 'Restricted bucket list' : 'Bucket inventory'" @open="emit('inventory')" />
      <WorkspaceMetric title="S3 credential" :value="capabilityNames[checkRows[0]?.state ?? 'unknown']" detail="Connection settings" @open="emit('connections')" />
    </div>

    <div class="manager-layout" :class="{ 'manager-overview': !inventory, 'manager-detail': !!bucket }">
      <WorkspacePanel :title="inventory ? 'Bucket inventory' : 'Buckets'" :description="inventory ? profile.name : 'Visible to the selected S3 identity'">
        <template #actions>
          <label v-if="inventory" class="manager-filter"><span class="sr-only">Filter buckets</span><input v-model="filter" type="search" placeholder="Find a bucket" /></label>
          <button v-else class="subtle" @click="emit('inventory')">View inventory <ArrowUpRight :size="15" /></button>
        </template>
        <Notice v-if="browser.bucketError" tone="error">{{ browser.bucketError }}</Notice>
        <p v-else-if="browser.bucketLoading" class="workspace-panel-message" role="status">Reading buckets…</p>
        <div v-else-if="shownBuckets.length" class="workspace-table">
          <table>
            <thead><tr><th scope="col">Bucket</th><th scope="col">Created</th><th scope="col"><span class="sr-only">Details</span></th></tr></thead>
            <tbody>
              <tr v-for="item in shownBuckets" :key="item.name" :class="{ 'workspace-row-active': item.name === bucket }">
                <th scope="row"><button class="subtle bucket-record" :aria-current="item.name === bucket ? 'location' : undefined" @click="emit('inspect', item.name)"><Database :size="16" />{{ item.name }}</button></th>
                <td>{{ formatDate(item.createdAt) }}</td>
                <td><button class="subtle icon-button" :aria-label="`Inspect ${item.name}`" @click="emit('inspect', item.name)"><ChevronRight :size="16" /></button></td>
              </tr>
            </tbody>
          </table>
        </div>
        <EmptyState v-else :icon="Database" :title="filter ? 'No matching buckets' : 'No visible buckets'"
          :description="filter ? 'Try a different bucket name.' : 'Create a bucket or check the known bucket list in Connections.'">
          <button v-if="filter" @click="filter = ''">Clear filter</button>
          <button v-else-if="!demo" @click="emit('connections')">Manage connections</button>
        </EmptyState>
      </WorkspacePanel>

      <WorkspacePanel v-if="!inventory" title="Credential checks" :description="`${verifiedCount} of ${checkRows.length} verified`">
        <template #chart><meter :value="verifiedCount" :max="checkRows.length" aria-label="Verified credential checks" /></template>
        <dl class="workspace-check-list">
          <div v-for="row in checkRows" :key="row.label"><dt>{{ row.label }}</dt><dd><span class="chip" :class="{ 'chip-active': row.state === 'detected' }">{{ capabilityNames[row.state] }}</span></dd></div>
        </dl>
      </WorkspacePanel>

      <FadeRegion>
        <WorkspaceDrawer v-if="inventory && bucket" :key="bucket" :title="bucket" @close="emit('inventory')">
          <WorkspaceTabs v-model="bucketTab" label="Bucket detail" :tabs="bucketTabs" />
          <FadeRegion>
            <div v-if="bucketTab === 'details'" class="workspace-drawer-content">
              <dl class="setup-summary">
                <div><dt>Connection</dt><dd>{{ profile.name }}</dd></div>
                <div><dt>Bucket</dt><dd><code>{{ bucket }}</code></dd></div>
                <div><dt>Created</dt><dd>{{ formatDate(openBucket?.createdAt ?? null) }}</dd></div>
              </dl>
              <div class="workspace-actions drawer-actions">
                <button v-if="browseAvailable" class="primary" @click="emit('browse', bucket)">Open in Browser <ArrowUpRight :size="16" /></button>
                <button @click="emit('tools', bucket)">Open storage tools</button>
              </div>
            </div>
            <BucketSettings v-else :connection="profile.id" :bucket="bucket" :demo="demo" @removed="bucketRemoved" />
          </FadeRegion>
        </WorkspaceDrawer>
      </FadeRegion>
    </div>
  </section>
</template>
