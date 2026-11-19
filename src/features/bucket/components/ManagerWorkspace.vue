<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ArrowUpRight, Plus, RefreshCw } from 'lucide-vue-next'
import { formatDate, formatSize } from '../../../common/format'
import WorkspaceDrawer from '../../../components/shell/WorkspaceDrawer.vue'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import BrowserWorkspace from '../../browser/components/BrowserWorkspace.vue'
import { useBrowser } from '../../browser/store/browser'
import { identityAvailable, providerNames, type Connection } from '../../connection/model/connection'
import { useMonitoring } from '../../monitoring/composables/useMonitoring'
import { formatCount } from '../../monitoring/model/dashboard'
import { demoMonitoring } from '../../monitoring/model/demo'
import BucketInventory from './BucketInventory.vue'
import BucketQuota from './BucketQuota.vue'
import BucketSettings from './BucketSettings.vue'
import BucketUsage from './BucketUsage.vue'

type BucketTab = 'overview' | 'configuration' | 'objects' | 'usage' | 'quota'

const props = defineProps<{ profile: Connection; bucket?: string; browseAvailable: boolean; demo: boolean; compact: boolean }>()
const emit = defineEmits<{ inventory: []; inspect: [bucket: string]; browse: [bucket: string]; create: []; connections: []; tools: [bucket: string]; job: [id: string] }>()
const browser = useBrowser()
const inventory = ref<InstanceType<typeof BucketInventory>>()
const settings = ref<InstanceType<typeof BucketSettings>>()
const quota = ref<InstanceType<typeof BucketQuota>>()
const bucketTab = ref<BucketTab>('overview')
const settingsVisited = ref(false)
const quotaAvailable = computed(() => props.profile.provider === 'ceph' && identityAvailable(props.profile, 'cephAdmin'))
const bucketTabs = computed(() => [
  { value: 'overview', label: 'Overview' },
  ...(props.browseAvailable ? [{ value: 'objects', label: 'Objects' } as const] : []),
  { value: 'usage', label: 'Usage' }, { value: 'configuration', label: 'Configuration' },
  ...(quotaAvailable.value ? [{ value: 'quota', label: 'Quota' } as const] : []),
] as const)
const embedded = computed(() => props.bucket ? { connection: props.profile.id, revision: props.profile.revision, bucket: props.bucket } : undefined)
const openBucket = computed(() => buckets.value.find(bucket => bucket.name === props.bucket))
const buckets = computed(() => browser.location.connection === props.profile.id ? browser.buckets : [])
const { view, pending, error, run } = useMonitoring(computed(() => props.profile))
const monitoring = computed(() => props.demo ? demoMonitoring : view.value)
const samples = computed(() => monitoring.value?.usage?.buckets ?? [])
const sample = computed(() => samples.value.find(item => item.bucket === props.bucket))
const dirty = computed(() => !!(settings.value?.dirty || quota.value?.dirty))
const busy = computed(() => !!(settings.value?.busy || quota.value?.saving))

watch(() => props.bucket, () => {
  bucketTab.value = 'overview'
  settingsVisited.value = false
})

watch(bucketTab, tab => {
  if (tab === 'configuration') settingsVisited.value = true
})

async function refresh(): Promise<void> {
  inventory.value?.refresh()
  await Promise.all([browser.loadBuckets(), settings.value?.refresh(), quota.value?.refresh(), run('cached')])
}

function changed(): void {
  if (props.bucket) inventory.value?.invalidate(props.bucket)
}

async function bucketRemoved(): Promise<void> {
  discard()
  changed()
  emit('inventory')
  await browser.loadBuckets()
}

function discard(): void {
  settings.value?.discard()
  quota.value?.discard()
}

defineExpose({ dirty, busy, discard })
</script>

<template>
  <section class="workspace-page manager-workspace" :class="{ 'workspace-compact': compact }">
    <WorkspaceHeader :title="bucket || 'Buckets'"
      :description="`${profile.name} · ${providerNames[profile.provider]} · ${profile.region}`">
      <template #actions>
        <button :disabled="busy || browser.bucketLoading" @click="refresh"><RefreshCw :size="16" :class="{ spinning: browser.bucketLoading }" /> Refresh</button>
        <button v-if="!bucket" class="primary" :disabled="demo" @click="emit('create')"><Plus :size="16" /> New bucket</button>
        <button v-else-if="browseAvailable" class="primary" @click="emit('browse', bucket)">Open in Browser <ArrowUpRight :size="16" /></button>
      </template>
    </WorkspaceHeader>
    <BucketInventory v-show="!bucket" ref="inventory" :connection="profile.id" :buckets="buckets" :samples="samples"
      :loading="browser.bucketLoading" :error="browser.bucketError"
      @inspect="emit('inspect', $event)" @connections="emit('connections')" />
    <FadeRegion>
      <WorkspaceDrawer v-if="bucket" :key="bucket" title="Bucket details" @close="emit('inventory')">
        <WorkspaceTabs v-model="bucketTab" label="Bucket detail" :tabs="bucketTabs" />
        <FadeRegion>
          <div v-show="bucketTab === 'overview'" class="workspace-drawer-content">
            <dl class="setup-summary">
              <div><dt>Connection</dt><dd>{{ profile.name }}</dd></div>
              <div><dt>Endpoint region</dt><dd>{{ profile.region }}</dd></div>
              <div><dt>Created</dt><dd>{{ formatDate(openBucket?.createdAt) }}</dd></div>
              <div><dt>Current storage</dt><dd>{{ formatSize(sample?.usage.value?.currentBytes) }}</dd></div>
              <div><dt>Objects</dt><dd>{{ formatCount(sample?.usage.value?.currentObjects) }}</dd></div>
            </dl>
            <div class="workspace-actions drawer-actions">
              <button v-if="browseAvailable" @click="bucketTab = 'objects'">Browse objects here</button>
              <button @click="emit('tools', bucket)">Open storage tools</button>
            </div>
          </div>
        </FadeRegion>
        <FadeRegion>
          <BrowserWorkspace v-if="bucketTab === 'objects' && embedded" :embedded="embedded" :demo="demo" :compact="true" @job="emit('job', $event)" />
        </FadeRegion>
        <FadeRegion>
          <div v-if="bucketTab === 'usage'">
            <Notice v-if="error" tone="error">{{ error }}</Notice>
            <BucketUsage :sample="sample" :pending="!!pending" :stale="!!monitoring?.stale" :demo="demo" @collect="run('collect')" />
          </div>
        </FadeRegion>
        <FadeRegion>
          <BucketSettings v-if="settingsVisited" v-show="bucketTab === 'configuration'" ref="settings" :connection="profile.id"
            :bucket="bucket" :demo="demo" managed @removed="bucketRemoved" @changed="changed" />
        </FadeRegion>
        <FadeRegion>
          <BucketQuota v-if="quotaAvailable" v-show="bucketTab === 'quota'" ref="quota" :profile="profile"
            :bucket="bucket" :active="bucketTab === 'quota'" @changed="run('cached')" />
        </FadeRegion>
      </WorkspaceDrawer>
    </FadeRegion>
  </section>
</template>

<style>
.bucket-table-controls { display: flex; align-items: center; gap: 8px; padding: 11px 14px; flex-wrap: wrap; }
.bucket-table-controls label { width: min(100%, 260px); }
.bucket-table-controls .select-field { width: min(100%, 160px); }
.bucket-column-options { display: grid; grid-template-columns: repeat(auto-fit, minmax(143px, 1fr)); gap: 6px 12px; margin: 0 14px 12px; padding: 10px; border: 1px solid var(--line); border-radius: 7px; background: var(--surface-alt); }
.bucket-column-options label { font-size: 12px; }
.bucket-detail-heading { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; }
.bucket-detail-heading .select-field { width: min(100%, 185px); }
.bucket-settings-group { display: grid; gap: 2px; margin-bottom: 6px; }
.bucket-settings-group h3 { margin: 3px 8px; color: var(--muted); font-size: 12px; }
.bucket-settings-group button { width: 100%; justify-content: space-between; gap: 6px; }
.bucket-settings-group small { color: var(--muted); font-size: 10px; text-align: end; }
.bucket-read-error { color: var(--danger); white-space: normal; overflow-wrap: anywhere; }
.literal-value { white-space: pre-wrap; }
.bucket-raw-rule { max-height: 60vh; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; background: var(--surface-alt); padding: 12px; }
.lifecycle-quick-actions { display: grid; gap: 6px; padding: 9px 11px; border: 1px solid var(--line); border-radius: 7px; }
.lifecycle-quick-actions > div { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.manager-workspace .bucket-configuration .configuration-grid { grid-template-columns: minmax(190px, 24%) minmax(0, 1fr); }
.manager-workspace .workspace-drawer > .workspace-drawer-content > .setup-summary { margin: 12px 14px 0; }
.manager-workspace .workspace-tabs { flex-wrap: wrap; }
@media (max-width: 770px) {
  .manager-workspace .bucket-configuration .configuration-grid { grid-template-columns: minmax(0, 1fr); }
  .bucket-settings-group { min-width: 145px; align-content: start; }
}
</style>
