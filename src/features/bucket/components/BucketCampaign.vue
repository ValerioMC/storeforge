<script setup lang="ts">
import { computed, ref, shallowRef, watch } from 'vue'
import { ArrowLeft, Clipboard, Download, RefreshCw, Upload } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { bucketsApi } from '../api/buckets'
import { campaignGateway } from '../api/campaigns'
import { useBucketCampaign, type CampaignRequest } from '../composables/useBucketCampaign'
import type { BucketQuota } from '../model/bucket'
import { campaignItemLabel, campaignSections, campaignStatuses, captureCampaign, quotaLimit, readCampaignBackup,
  snapshotPatch, snapshotSections, validateConfigIntent, writeCampaignBackup, type CampaignItem, type CampaignPatch,
  type CampaignSection, type CampaignSnapshot, type ConfigIntent } from '../model/campaign'
import type { WorkbenchRow } from '../model/workbench'
import BucketCampaignEdit, { type CampaignEdit } from './BucketCampaignEdit.vue'

const props = defineProps<{ rows: WorkbenchRow[]; selection: WorkbenchRow[]; clipboard?: CampaignSnapshot }>()
const emit = defineEmits<{ close: []; clipboard: [value: CampaignSnapshot]; result: [row: WorkbenchRow, item: CampaignItem] }>()
const targets = props.selection.slice()
const modes = [{ value: 'manual', label: 'Edit settings' }, { value: 'copy', label: 'Copy from bucket' },
  { value: 'paste', label: 'Paste clipboard or backup' }, { value: 'usage', label: 'Recalculate utilization' }] as const
const mappings = [{ value: 'single', label: 'One source to many targets' }, { value: 'paired', label: 'One source per target' }] as const
const mode = ref<typeof modes[number]['value']>(props.clipboard ? 'paste' : 'manual')
const mapping = ref<typeof mappings[number]['value']>('single')
const copySource = ref('')
const pairs = ref<Record<string, string>>({})
const captured = shallowRef(props.clipboard)
const sections = ref<CampaignSection[]>(props.clipboard ? snapshotSections(props.clipboard) : ['versioning'])
const sourceReports = shallowRef<{ row: WorkbenchRow; items: CampaignItem[] }[]>([])
const editing = ref<CampaignEdit>({
  section: 'versioning', operation: 'replace', document: '', removeIds: '', versioning: 'Enabled',
  setQuotaEnabled: false, setMaxBytes: false, setMaxObjects: false, quotaEnabled: true, maxBytes: '', maxObjects: '',
})
const error = ref('')
const preparing = ref(false)
const exporting = ref(false)
const importPending = ref(false)
const { targets: reviews, confirmed, stage, running, progress, eligible, selected, retryable, setConfirmed, confirmEligible, preview, apply, retry, edit } =
  useBucketCampaign(campaignGateway, (row, item) => emit('result', row, item))
const busy = computed(() => preparing.value || running.value || exporting.value || importPending.value)
const sourceOptions = computed(() => props.rows.map(row => ({ value: row.key, label: targetLabel(row) })))
const sectionOptions = computed(() => mode.value === 'paste' && captured.value
  ? campaignSections.filter(section => snapshotSections(captured.value!).includes(section.value)) : campaignSections)
const previewReady = computed(() => {
  if (mode.value === 'manual' || mode.value === 'usage') return true
  if (!sections.value.length) return false
  if (mode.value === 'paste') return !!captured.value
  if (mapping.value === 'single') return !!copySource.value
  return targets.every(row => !!pairs.value[row.key])
})
const demoOnly = targets.every(row => row.profile.id === 'demo')

watch(mode, value => {
  error.value = ''
  sourceReports.value = []
  if (value === 'paste') sections.value = captured.value ? snapshotSections(captured.value) : []
  else if (value === 'copy') sections.value = ['versioning']
})

function targetLabel(row: WorkbenchRow): string {
  return [row.name, row.profile.name, row.tenant || 'Default tenant', row.instance].filter(Boolean).join(' · ')
}

function setPair(key: string, source: string): void {
  pairs.value = { ...pairs.value, [key]: source }
}

function sourceRow(key: string): WorkbenchRow {
  const row = props.rows.find(candidate => candidate.key === key)
  if (!row) throw new Error('The source bucket is no longer in the inventory.')
  return row
}

function manualPatch(): CampaignPatch {
  const value = editing.value
  if (value.section === 'quota') {
    const quota: Partial<BucketQuota> = {}
    if (value.setQuotaEnabled) quota.enabled = value.quotaEnabled
    if (value.setMaxBytes) quota.maxBytes = quotaLimit(value.maxBytes.trim())
    if (value.setMaxObjects) quota.maxObjects = quotaLimit(value.maxObjects.trim())
    if (!Object.keys(quota).length) throw new Error('Select at least one quota field to update.')
    return { sections: [], quota }
  }
  const document = value.section === 'versioning'
    ? `<VersioningConfiguration><Status>${value.versioning}</Status></VersioningConfiguration>` : value.document
  const intent: ConfigIntent = {
    section: value.section, operation: value.operation, document,
    removeIds: value.removeIds.split(/\r?\n/).map(id => id.trim()).filter(Boolean),
  }
  validateConfigIntent(intent)
  return { sections: [intent] }
}

function saveClipboard(source: CampaignSnapshot): void {
  if (!snapshotSections(source).length) throw new Error('None of the selected source settings are available to copy.')
  captured.value = source
  emit('clipboard', source)
}

async function captureSource(row: WorkbenchRow): Promise<CampaignSnapshot> {
  const { snapshot, items } = await captureCampaign(row, sections.value, campaignGateway)
  sourceReports.value = [...sourceReports.value, { row, items }]
  return snapshot
}

async function copyToClipboard(): Promise<void> {
  preparing.value = true
  error.value = ''
  sourceReports.value = []
  try {
    const snapshot = await captureSource(sourceRow(copySource.value))
    saveClipboard(snapshot)
    if (snapshotSections(snapshot).length === sections.value.length) mode.value = 'paste'
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    preparing.value = false
  }
}

async function prepareRequests(): Promise<CampaignRequest[]> {
  if (mode.value === 'manual' || mode.value === 'usage') {
    const patch = mode.value === 'usage' ? { sections: [], usage: true } : manualPatch()
    return targets.map(row => ({ row, patch }))
  }
  if (mode.value === 'paste') {
    const source = captured.value
    if (!source) throw new Error('Load a configuration before previewing.')
    return targets.map(row => ({ row, patch: snapshotPatch(source, sections.value, row) }))
  }
  if (mapping.value === 'single') {
    const source = await captureSource(sourceRow(copySource.value))
    return targets.map(row => ({ row, patch: snapshotPatch(source, sections.value, row) }))
  }
  const keys = targets.map(row => pairs.value[row.key] ?? '')
  if (new Set(keys).size !== keys.length) throw new Error('Choose a distinct source bucket for each target.')
  const requests: CampaignRequest[] = []
  for (const row of targets) {
    const source = await captureSource(sourceRow(pairs.value[row.key] ?? ''))
    requests.push({ row, patch: snapshotPatch(source, sections.value, row) })
  }
  return requests
}

async function buildPreview(): Promise<void> {
  if (busy.value || !previewReady.value) return
  preparing.value = true
  error.value = ''
  sourceReports.value = []
  try {
    await preview(await prepareRequests())
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    preparing.value = false
  }
}

async function importBackup(): Promise<void> {
  importPending.value = true
  error.value = ''
  try {
    const source = await bucketsApi.importBackup()
    if (source === null) return
    const snapshot = readCampaignBackup(source)
    saveClipboard(snapshot)
    sections.value = snapshotSections(snapshot)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    importPending.value = false
  }
}

async function backup(): Promise<void> {
  if (!captured.value || !sections.value.length) return
  exporting.value = true
  error.value = ''
  const source = captured.value
  const snapshot: CampaignSnapshot = {
    name: source.name, quota: sections.value.includes('quota') ? source.quota : undefined,
    documents: Object.fromEntries(Object.entries(source.documents).filter(([section]) =>
      sections.value.some(value => value === section))),
  }
  try {
    await bucketsApi.exportBackup(source.name, writeCampaignBackup(snapshot))
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    exporting.value = false
  }
}

function checked(event: Event): boolean {
  return (event.currentTarget as HTMLInputElement).checked
}

function close(): void {
  if (!busy.value) emit('close')
}

defineExpose({ busy })
</script>

<template>
  <Modal title="Bucket campaign" drawer @close="close">
    <div class="modal-form campaign">
      <div class="campaign-scope">
        <strong>{{ targets.length }} {{ targets.length === 1 ? 'target bucket' : 'target buckets' }}</strong>
        <span v-if="demoOnly" class="chip">Read-only demo</span>
      </div>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <FadeRegion>
        <form v-if="stage === 'draft'" class="panel-form campaign-draft" :inert="busy" @submit.prevent="buildPreview">
          <label>Campaign action <AppSelect v-model="mode" :options="modes" aria-label="Campaign action" /></label>
          <BucketCampaignEdit v-if="mode === 'manual'" v-model="editing" />
          <p v-else-if="mode === 'usage'" class="muted">Scan the selected buckets and report objects, bytes and listing coverage.</p>
          <div v-else class="panel-form">
            <template v-if="mode === 'copy'">
              <label>Source mapping <AppSelect v-model="mapping" :options="mappings" aria-label="Source mapping" /></label>
              <label v-if="mapping === 'single'">Source bucket
                <AppSelect v-model="copySource" :options="sourceOptions" aria-label="Source bucket" placeholder="Choose a source bucket" />
              </label>
              <div v-else class="campaign-mapping">
                <label v-for="(row, index) in targets" :key="row.key">{{ targetLabel(row) }}
                  <AppSelect :model-value="pairs[row.key] ?? ''" :options="sourceOptions" :aria-label="`Source for target ${index + 1}`"
                    placeholder="Choose a source bucket" @update:model-value="value => setPair(row.key, value)" />
                </label>
              </div>
            </template>
            <div v-else class="campaign-scope">
              <span>{{ captured ? `Clipboard source · ${captured.name}` : 'The configuration clipboard is empty.' }}</span>
              <div class="panel-actions">
                <button type="button" :disabled="demoOnly" @click="importBackup"><Upload :size="15" /> Import backup</button>
                <button v-if="captured" type="button" :disabled="!sections.length || demoOnly" @click="backup"><Download :size="15" /> Export backup</button>
              </div>
            </div>
            <fieldset v-if="sectionOptions.length">
              <legend>Settings to include</legend>
              <div class="campaign-settings">
                <label v-for="section in sectionOptions" :key="section.value" class="check-label">
                  <input v-model="sections" type="checkbox" :value="section.value" />{{ section.label }}
                </label>
              </div>
            </fieldset>
            <p v-if="mode === 'copy' || captured" class="muted">Lifecycle and notification rules merge with existing settings. Bucket policy resources that reference the source bucket are mapped to the target bucket.</p>
          </div>
          <div class="modal-actions">
            <button v-if="mode === 'copy' && mapping === 'single'" type="button" :disabled="!copySource || !sections.length" @click="copyToClipboard">
              <Clipboard :size="15" /> Copy to clipboard
            </button>
            <button type="submit" class="primary" :disabled="!previewReady">Preview changes</button>
          </div>
        </form>
        <div v-else class="panel-form">
          <div class="campaign-scope">
            <h3>{{ stage === 'preview' ? 'Review changes' : 'Results' }}</h3>
            <button type="button" :disabled="busy" @click="edit"><ArrowLeft :size="15" /> Edit campaign</button>
          </div>
          <div v-for="review in reviews" :key="review.row.key" class="campaign-target">
            <div class="campaign-target-header">
              <strong>{{ targetLabel(review.row) }}</strong>
              <label v-if="stage === 'preview' && eligible.includes(review)" class="check-label">
                <input type="checkbox" :aria-label="`Confirm ${targetLabel(review.row)}`"
                  :checked="confirmed.has(review.row.key)" :disabled="busy" @change="setConfirmed(review.row.key, checked($event))" /> Confirm target
              </label>

            </div>
            <p v-if="!review.items.length" class="muted">No selected source settings are configured.</p>
            <div v-for="item in review.items" :key="item.section" class="campaign-item">
              <span>{{ campaignItemLabel(item.section) }}</span>
              <span class="chip" :data-result="item.status">{{ campaignStatuses[item.status] }}</span>
              <span class="muted">{{ item.summary }}</span>
              <details v-if="item.before !== undefined || item.after !== undefined" class="campaign-diff">
                <summary>View settings before and after</summary>
                <div><div><strong>Current</strong><pre>{{ item.before }}</pre></div><div><strong>Proposed</strong><pre>{{ item.after }}</pre></div></div>
              </details>
            </div>
          </div>
          <div class="modal-actions">
            <button v-if="stage === 'preview'" type="button" :disabled="busy || !eligible.length" @click="confirmEligible">Confirm eligible targets</button>
            <button v-if="retryable.length" type="button" :disabled="busy" @click="retry"><RefreshCw :size="15" /> Preview failed targets again</button>
            <button v-if="stage === 'preview'" type="button" class="primary" :disabled="busy || !selected.length" @click="apply">
              Apply to {{ selected.length }} {{ selected.length === 1 ? 'target' : 'targets' }}
            </button>
          </div>
        </div>
      </FadeRegion>
      <details v-if="sourceReports.length" class="campaign-diff">
        <summary>Source settings</summary>
        <div v-for="{ row, items } in sourceReports" :key="row.key" class="campaign-target">
          <strong>{{ targetLabel(row) }}</strong>
          <div v-for="item in items" :key="item.section" class="campaign-item">
            <span>{{ campaignItemLabel(item.section) }}</span><span class="muted">{{ item.summary }}</span>
          </div>
        </div>
      </details>
      <p v-if="running" role="status" class="muted">{{ progress.complete }} of {{ progress.total }} targets {{ stage === 'results' ? 'processed' : 'read' }}</p>
      <p v-else-if="preparing" role="status" class="muted">Preparing campaign settings…</p>
    </div>
  </Modal>
</template>

<style scoped>
.campaign-scope, .campaign-target-header { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px; }
.campaign-scope, .campaign-target-header strong { overflow-wrap: anywhere; }
.campaign-draft > label, .campaign-draft .panel-form > label { display: grid; gap: 5px; }
.campaign-settings, .campaign-mapping { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px 14px; }
.campaign-mapping label { display: grid; gap: 5px; overflow-wrap: anywhere; }
.campaign-target { border: 1px solid var(--line); border-radius: var(--radius); padding: 10px 12px; overflow-wrap: anywhere; display: grid; gap: 8px; }
.campaign-item { display: flex; flex-wrap: wrap; gap: 6px 9px; align-items: baseline; }
.campaign-item > span:first-child { font-weight: 630; }
.campaign-diff { flex-basis: 100%; }
.campaign-diff summary { cursor: pointer; color: var(--accent-deep); padding-block: 4px; }
.campaign-diff > div:not(.campaign-target) { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; padding-block: 5px; }
.campaign-diff > div + div { margin-top: 6px; }
.campaign-diff pre { white-space: pre-wrap; overflow-wrap: anywhere; background: var(--surface-alt); padding: 8px; max-height: 220px; overflow: auto; font-size: 12.5px; }
.chip[data-result="error"], .chip[data-result="denied"] { color: var(--danger); }
.chip[data-result="applied"] { color: var(--success); }
@media (max-width: 570px) {
  .campaign-settings, .campaign-mapping, .campaign-diff > div:not(.campaign-target) { grid-template-columns: 1fr; }
}
</style>
