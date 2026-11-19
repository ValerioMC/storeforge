<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Database, RefreshCw, SlidersHorizontal } from 'lucide-vue-next'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { useBrowser } from '../../browser/store/browser'
import { useConfigurationInventory } from '../composables/useConfigurationInventory'
import { extractRules, ruleFeatures, type BucketRule, type RuleFeature } from '../model/workspace'

const props = defineProps<{ connection: string }>()
const emit = defineEmits<{ inspect: [bucket: string] }>()
const browser = useBrowser()
const search = ref('')
const feature = ref<RuleFeature>('lifecycle')
const selected = ref<{ bucket: string; rule: BucketRule }>()
const state = ref<'all' | 'configured' | 'missing' | 'error' | 'Disabled'>('all')
const states = computed(() => [
  { value: 'all', label: 'Any state' }, { value: 'configured', label: 'With rules' },
  { value: 'missing', label: 'Without rules' }, { value: 'error', label: 'Read error' },
  ...(feature.value === 'lifecycle' ? [{ value: 'Disabled', label: 'Disabled rules' } as const] : []),
] as const)
const names = computed(() => browser.location.connection === props.connection ? browser.buckets.map(item => item.name) : [])
const { rows, pending, refresh } = useConfigurationInventory(computed(() => props.connection), names, computed(() => [feature.value]))
const table = computed(() => names.value.flatMap(bucket => {
  const reading = rows.get(bucket)?.[feature.value]
  if (!reading) return []
  const rules = reading.status === 'error' ? [] : extractRules(feature.value, reading.document)
  const query = search.value.trim().toLowerCase()
  const matched = rules.filter(rule => [bucket, rule.name, rule.detail, rule.status].some(value => value.toLowerCase().includes(query)))
  if (query && !bucket.toLowerCase().includes(query) && !matched.length) return []
  if (state.value === 'Disabled') {
    const disabled = matched.filter(rule => rule.status === 'Disabled')
    return disabled.length ? [{ bucket, reading, rules: disabled }] : []
  }
  const status = reading.status === 'error' ? 'error' : rules.length ? 'configured' : 'missing'
  return state.value === 'all' || state.value === status ? [{ bucket, reading, rules: matched }] : []
}))

watch(feature, () => { state.value = 'all' })

async function refreshRules(): Promise<void> {
  await browser.loadBuckets()
  refresh()
}
</script>

<template>
  <section class="workspace-page manager-workspace">
    <WorkspaceHeader title="Feature rules" description="Inspect bucket rules across the selected connection.">
      <template #actions><button :disabled="pending || browser.bucketLoading" @click="refreshRules"><RefreshCw :size="16" /> Refresh</button></template>
    </WorkspaceHeader>
    <WorkspacePanel :title="ruleFeatures.find(item => item.value === feature)!.label" :description="`${table.length} of ${names.length} buckets`">
      <div class="bucket-table-controls">
        <AppSelect v-model="feature" :options="ruleFeatures" aria-label="Feature" />
        <AppSelect v-model="state" :options="states" aria-label="Rule state" />
        <label><span class="sr-only">Find rules</span><input v-model="search" type="search" placeholder="Find by bucket or rule" /></label>
      </div>
      <Notice v-if="browser.bucketError" tone="error">{{ browser.bucketError }}</Notice>
      <p v-else-if="pending || browser.bucketLoading" class="workspace-panel-message" role="status">Reading bucket rules…</p>
      <div v-if="table.length" class="workspace-table">
        <table>
          <thead><tr><th scope="col">Bucket</th><th scope="col">Rule</th><th scope="col">Value or scope</th><th scope="col">State</th><th scope="col"><span class="sr-only">Inspect</span></th></tr></thead>
          <tbody>
            <template v-for="{ bucket, reading, rules } in table" :key="bucket">
              <tr v-if="!rules.length">
                <th scope="row"><button class="subtle bucket-record" @click="emit('inspect', bucket)"><Database :size="15" /> {{ bucket }}</button></th>
                <td colspan="4"><span v-if="reading.status === 'error'" class="bucket-read-error">{{ reading.message }}</span><span v-else class="muted">No rules</span></td>
              </tr>
              <tr v-for="(rule, index) in rules" :key="index">
                <th scope="row"><button class="subtle bucket-record" @click="emit('inspect', bucket)"><Database :size="15" /> {{ bucket }}</button></th>
                <td class="literal-value">{{ rule.name }}</td><td class="literal-value">{{ rule.detail || '—' }}</td><td>{{ rule.status || '—' }}</td>
                <td><button type="button" @click="selected = { bucket, rule }">Inspect rule</button></td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
      <EmptyState v-else-if="!browser.bucketError && !browser.bucketLoading && !pending" :icon="SlidersHorizontal" title="No matching rules" />
    </WorkspacePanel>
    <Modal v-if="selected" :title="`${selected.bucket} / ${selected.rule.name}`" wide @close="selected = undefined">
      <div class="modal-form"><pre class="bucket-raw-rule">{{ selected.rule.raw }}</pre></div>
    </Modal>
  </section>
</template>
