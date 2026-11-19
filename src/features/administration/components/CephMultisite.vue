<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { RefreshCw } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import { identityAvailable, type Connection } from '../../connection/model/connection'
import { readMultisiteTopology } from '../api/ceph-multisite'
import { availabilityLabels, profileScope, zoneEndpoints, type BucketIdentity, type Topology } from '../model/ceph-multisite'
import CephJournal from './CephJournal.vue'
import CephReplication from './CephReplication.vue'
import CephTopology from './CephTopology.vue'

const props = defineProps<{ profile: Connection }>()
const tabs = [{ value: 'topology', label: 'Topology' }, { value: 'replication', label: 'Bucket replication' }, { value: 'journals', label: 'Journals' }] as const
const tab = ref<typeof tabs[number]['value']>('topology')
const topology = ref<Topology>()
const loading = ref(false)
const error = ref('')
const preset = ref<{ gateway: string; bucket: BucketIdentity; generation: string }>()
const journalKey = ref(0)
const authorized = computed(() => identityAvailable(props.profile, 'cephAdmin') && props.profile.id !== 'demo')
const period = computed(() => topology.value?.period.data)
const endpoints = computed(() => topology.value && period.value ? zoneEndpoints(period.value, topology.value.registrations) : [])
const local = computed(() => topology.value?.registrations.find(entry => entry.scope.connection === props.profile.id)?.local)
let request = 0

watch(() => [props.profile.id, props.profile.revision], () => { void refresh() }, { immediate: true })

async function refresh(): Promise<void> {
  if (!authorized.value) return
  const current = ++request
  const expected = profileScope(props.profile)
  loading.value = true
  topology.value = undefined
  preset.value = undefined
  error.value = ''
  try {
    const result = await readMultisiteTopology(expected)
    if (current === request && result.anchor.connection === expected.connection && result.anchor.revision === expected.revision) {
      topology.value = result
    }
  } catch (failure) {
    if (current === request) error.value = errorMessage(failure)
  } finally {
    if (current === request) loading.value = false
  }
}

function openJournal(gateway: string, bucket: BucketIdentity, generation: string): void {
  preset.value = { gateway, bucket, generation }
  journalKey.value++
  tab.value = 'journals'
}
</script>

<template>
  <section class="workspace-page multisite-page">
    <WorkspaceHeader title="Ceph multisite" :description="`${profile.name} · ${local?.data?.name || profile.services?.cephAdmin?.endpoint || profile.endpoint}`">
      <template #actions><button type="button" :disabled="loading || !authorized" @click="refresh"><RefreshCw :size="15" :class="{ spinning: loading }" /> Refresh topology</button></template>
    </WorkspaceHeader>
    <Notice v-if="!authorized">This workspace requires a verified Ceph Admin identity.</Notice>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <p v-if="loading" role="status" class="muted">Discovering registered gateways and the active period…</p>
    <Notice v-else-if="topology && !period" :tone="topology.period.availability === 'notApplicable' ? 'info' : 'error'">
      <template v-if="topology.period.availability === 'notApplicable'">This gateway does not report a multisite realm.</template>
      <template v-else>Topology {{ availabilityLabels[topology.period.availability].toLowerCase() }}<template v-if="topology.period.status"> · HTTP {{ topology.period.status }}</template>.</template>
    </Notice>
    <template v-if="topology && period">
      <WorkspaceTabs v-model="tab" :tabs="tabs" label="Multisite views" />
      <div class="multisite-stack">
        <FadeRegion>
          <div v-show="tab === 'topology'" class="multisite-stack"><CephTopology :topology="topology" :period="period" :endpoints="endpoints" /></div>
        </FadeRegion>
        <FadeRegion>
          <div v-show="tab === 'replication'" class="multisite-stack"><CephReplication :endpoints="endpoints" @journal="openJournal" /></div>
        </FadeRegion>
        <FadeRegion>
          <div v-show="tab === 'journals'" class="multisite-stack"><CephJournal :key="journalKey" :period="period.id" :endpoints="endpoints" :preset="preset" /></div>
        </FadeRegion>
      </div>
    </template>
  </section>
</template>

<style scoped>
.multisite-page, .multisite-stack { display: grid; gap: 14px; min-width: 0; }
.multisite-page .page-header, .multisite-page .workspace-tabs { margin-bottom: 0; }
</style>
