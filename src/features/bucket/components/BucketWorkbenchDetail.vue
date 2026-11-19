<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ChevronLeft, ChevronRight, Link2 } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import { formatDate, formatSize } from '../../../common/format'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import Notice from '../../../components/ui/Notice.vue'
import CephBucketActions from '../../administration/components/CephBucketActions.vue'
import { scopedBucketConfiguration } from '../api/workbench'
import type { Configuration, ConfigurationChange } from '../model/bucket'
import type { WorkbenchRow } from '../model/workbench'
import BucketSettings from './BucketSettings.vue'

const props = defineProps<{ row: WorkbenchRow; tags: readonly string[]; previous?: string; next?: string; link: string }>()
const emit = defineEmits<{ inspect: [key: string]; classify: [tags: string]; changed: []; inventoryChanged: [] }>()
const tabs = [{ value: 'overview', label: 'Overview' }, { value: 'configuration', label: 'Configuration' }] as const
const tab = ref<typeof tabs[number]['value']>('configuration')
const settings = ref<InstanceType<typeof BucketSettings>>()
const draft = ref('')
const feedback = ref('')
const feedbackError = ref(false)
const dirty = computed(() => !!settings.value?.dirty)
const busy = computed(() => !!settings.value?.busy)
const title = computed(() => [props.row.profile.name, props.row.tenant || 'Default tenant', props.row.owner].filter(Boolean).join(' · '))

watch(() => props.tags, value => { draft.value = value.join(', ') }, { immediate: true })

function request(section: Configuration, change: ConfigurationChange): Promise<string> {
  return scopedBucketConfiguration(props.row.target, section, change)
}

function classify(): void {
  emit('classify', draft.value)
}

async function copyLink(): Promise<void> {
  try {
    await navigator.clipboard.writeText(props.link)
    feedback.value = 'Bucket link copied.'
    feedbackError.value = false
  } catch (failure) {
    feedback.value = errorMessage(failure)
    feedbackError.value = true
  }
}

function discard(): void {
  settings.value?.discard()
}

defineExpose({ dirty, busy, discard })
</script>

<template>
  <div class="workbench-detail">
    <div class="detail-scope">
      <div><strong>{{ title }}</strong><p class="muted">{{ row.profile.endpoint || row.profile.region }}<template v-if="row.instance"> · {{ row.instance }}</template></p></div>
      <div class="panel-actions">
        <button class="icon-button" type="button" aria-label="Previous bucket" :disabled="!previous || busy" @click="previous && emit('inspect', previous)"><ChevronLeft :size="16" /></button>
        <button class="icon-button" type="button" aria-label="Next bucket" :disabled="!next || busy" @click="next && emit('inspect', next)"><ChevronRight :size="16" /></button>
        <button type="button" @click="copyLink"><Link2 :size="16" /> Copy link</button>
      </div>
    </div>
    <CephBucketActions v-if="row.target.source === 'cephAdmin'" :row="row" @changed="emit('inventoryChanged')" />
    <Notice v-if="feedback" :tone="feedbackError ? 'error' : 'info'">{{ feedback }}</Notice>
    <WorkspaceTabs v-model="tab" :tabs="tabs" label="Bucket details sections" />
    <div v-show="tab === 'overview'" class="panel-form">
      <dl class="setup-summary">
        <div><dt>Created</dt><dd>{{ formatDate(row.createdAt) }}</dd></div>
        <div><dt>Storage</dt><dd>{{ formatSize(row.bytes) }}</dd></div>
        <div><dt>Objects</dt><dd>{{ row.objects?.toLocaleString() ?? 'Unavailable' }}</dd></div>
        <div><dt>Quota enforcement</dt><dd>{{ !row.quota ? 'Unavailable' : row.quota.enabled ? 'Enabled' : 'Disabled' }}</dd></div>
      </dl>
      <form class="panel-form local-tags" @submit.prevent="classify">
        <label>UI tags <input v-model="draft" placeholder="e.g. production, archive" :disabled="row.profile.id === 'demo'" /></label>
        <p class="muted">Use commas to separate workspace tags. S3 tags are under Configuration.</p>
        <button type="submit" :disabled="row.profile.id === 'demo' || draft === tags.join(', ')">Save UI tags</button>
      </form>
    </div>
    <div v-show="tab === 'configuration'">
      <Notice v-if="!row.configurationAvailable">This inventory row has no S3 configuration identity in its tenant.</Notice>
      <BucketSettings v-else ref="settings" :connection="row.profile.id" :bucket="row.name"
        :demo="row.profile.id === 'demo'" :scoped-request="request" managed @changed="emit('changed')" />
    </div>
  </div>
</template>

<style scoped>
.workbench-detail { display: grid; gap: 12px; }
.detail-scope { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 10px; }
.detail-scope > div:first-child { min-width: 0; overflow-wrap: anywhere; }
.local-tags { justify-items: start; }
.local-tags label { display: grid; gap: 5px; min-width: min(340px, 100%); }
</style>
