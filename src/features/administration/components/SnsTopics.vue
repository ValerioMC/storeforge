<script setup lang="ts">
import { computed, onScopeDispose, ref, watch } from 'vue'
import { ArrowLeft, ArrowUpRight, Plus } from 'lucide-vue-next'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { bucketsApi } from '../../bucket/api/buckets'
import type { Connection } from '../../connection/model/connection'
import { writeQuery } from '../api/iam'
import { createTopic, inspectTopic, listTopics, setTopicAttribute, snsError, topicPolicy } from '../api/resources'
import { validPolicy } from '../model/iam'
import type { Topic, TopicAttribute } from '../model/resources'

const props = defineProps<{ profile: Connection }>()
const emit = defineEmits<{ notifications: [bucket: string, arn: string] }>()
const tabs = [{ value: 'delivery', label: 'Delivery' }, { value: 'policy', label: 'Policy' }] as const
const tab = ref<'delivery' | 'policy'>('delivery')
const topics = ref<Topic[]>([])
const selected = ref<Topic>()
const search = ref('')
const loading = ref(false)
const busy = ref(false)
const error = ref('')
const detailError = ref('')
const creating = ref(false)
const removing = ref(false)
const name = ref('')
const endpoint = ref('')
const attributes = ref<TopicAttribute[]>([])
const attribute = ref('')
const value = ref('')
const policy = ref('')
const bucketOptions = ref<{ value: string; label: string }[]>([])
const bucket = ref('')
const bucketError = ref('')
const bucketLoading = ref(false)
const filtered = computed(() => topics.value.filter(topic => `${topic.name} ${topic.arn}`.toLowerCase().includes(search.value.trim().toLowerCase())))
const availableAttributes = computed(() => props.profile.provider === 'ceph' ? [
  { value: 'push-endpoint', label: 'Push endpoint' }, { value: 'OpaqueData', label: 'Opaque data' },
  { value: 'persistent', label: 'Persistent' }, { value: 'verify-ssl', label: 'Verify TLS certificate' },
  { value: 'time_to_live', label: 'Time to live' }, { value: 'max_retries', label: 'Maximum retries' },
  { value: 'retry_sleep_duration', label: 'Retry interval' },
] : [{ value: 'DisplayName', label: 'Display name' }, { value: 'DeliveryPolicy', label: 'Delivery policy JSON' }])
let request = 0
let detail = 0
let disposed = false
onScopeDispose(() => { disposed = true })
watch(() => props.profile.id, refresh, { immediate: true })

async function refresh(): Promise<void> {
  const current = ++request
  loading.value = true
  error.value = ''
  try {
    const result = await listTopics(props.profile.id)
    if (!disposed && request === current) topics.value = result
  } catch (failure) {
    if (!disposed && request === current) error.value = snsError(failure)
  } finally {
    if (!disposed && request === current) loading.value = false
  }
}

async function loadDetails(topic: Topic, current: number): Promise<void> {
  try {
    const result = await inspectTopic(props.profile.id, topic.arn)
    if (disposed || detail !== current) return
    attributes.value = result
    policy.value = topicPolicy(result)
    chooseAttribute(attribute.value)
    detailError.value = ''
  } catch (failure) {
    if (!disposed && detail === current) detailError.value = snsError(failure)
  }
}

async function loadBuckets(current: number): Promise<void> {
  bucketLoading.value = true
  bucketError.value = ''
  try {
    const result = await bucketsApi.list(props.profile.id)
    if (!disposed && detail === current) bucketOptions.value = result.map(item => ({ value: item.name, label: item.name }))
  } catch (failure) {
    if (!disposed && detail === current) {
      bucketOptions.value = props.profile.knownBuckets.map(name => ({ label: name, value: name }))
      bucketError.value = snsError(failure)
    }
  } finally {
    if (!disposed && detail === current) bucketLoading.value = false
  }
}

async function open(topic: Topic): Promise<void> {
  const current = ++detail
  selected.value = topic
  tab.value = 'delivery'
  error.value = ''
  detailError.value = ''
  attributes.value = []
  attribute.value = ''
  value.value = ''
  policy.value = ''
  bucket.value = ''
  busy.value = true
  void loadBuckets(current)
  await loadDetails(topic, current)
  if (!disposed && detail === current) busy.value = false
}

function close(): void {
  detail++
  selected.value = undefined
  error.value = ''
  busy.value = false
}

function chooseAttribute(next: string): void {
  attribute.value = next
  value.value = attributes.value.find(item => item.name === next)?.value ?? ''
}

async function create(): Promise<void> {
  if (!name.value.trim() || busy.value) return
  busy.value = true
  error.value = ''
  try {
    const topic = await createTopic(props.profile.id, name.value.trim(),
      endpoint.value.trim() ? { name: 'push-endpoint', value: endpoint.value.trim() } : undefined)
    if (disposed) return
    creating.value = false
    await open(topic)
    await refresh()
  } catch (failure) {
    if (!disposed) error.value = snsError(failure)
  } finally {
    if (!disposed) busy.value = false
  }
}

async function update(change: TopicAttribute): Promise<void> {
  const topic = selected.value
  if (!topic || busy.value || detailError.value) return
  const current = detail
  busy.value = true
  error.value = ''
  try {
    await setTopicAttribute(props.profile.id, topic.arn, change)
  } catch (failure) {
    if (!disposed && detail === current) error.value = snsError(failure)
  } finally {
    if (!disposed && detail === current) {
      await loadDetails(topic, current)
      busy.value = false
    }
  }
}

async function remove(): Promise<void> {
  const topic = selected.value
  if (!topic || busy.value) return
  busy.value = true
  error.value = ''
  removing.value = false
  try {
    await writeQuery(props.profile.id, 'DeleteTopic', { TopicArn: topic.arn })
    if (disposed) return
    close()
    await refresh()
  } catch (failure) {
    if (!disposed) error.value = snsError(failure)
  } finally {
    if (!disposed) busy.value = false
  }
}

function newTopic(): void {
  creating.value = true
  name.value = ''
  endpoint.value = ''
  error.value = ''
}
</script>

<template>
  <div class="panel-form">
    <template v-if="!selected">
      <div class="stage-heading"><h2>SNS topics</h2><button class="primary" @click="newTopic"><Plus :size="16" /> Create topic</button></div>
      <div class="form-grid">
        <label>Search topics<input v-model="search" type="search" /></label>
        <div class="panel-actions"><button :disabled="loading" @click="refresh">Refresh</button></div>
      </div>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <p v-else-if="loading" role="status">Loading topics…</p>
      <p v-else-if="!filtered.length" class="muted">No matching topics.</p>
      <div v-for="topic in filtered" :key="topic.arn" class="settings-row">
        <div class="settings-description"><strong>{{ topic.name }}</strong><p>{{ topic.arn }}</p></div>
        <button :aria-label="`Open ${topic.name}`" @click="open(topic)">Open</button>
      </div>
    </template>
    <template v-else>
      <button :disabled="busy" @click="close"><ArrowLeft :size="15" /> SNS topics</button>
      <div class="stage-heading">
        <div><h2>{{ selected.name }}</h2><p>{{ selected.arn }}</p></div>
        <button :disabled="busy" @click="removing = true">Delete topic</button>
      </div>
      <Notice v-if="error || detailError" tone="error">{{ error || detailError }}</Notice>
      <button v-if="detailError" :disabled="busy" @click="open(selected)">Reload topic</button>
      <WorkspaceTabs v-model="tab" :tabs="tabs" label="SNS topic sections" />
      <FadeRegion>
        <div :key="tab" class="panel-form">
          <template v-if="tab === 'delivery'">
            <dl v-if="attributes.length" class="topic-attributes">
              <template v-for="item in attributes.filter(row => row.name !== 'Policy')" :key="item.name">
                <dt>{{ item.name }}</dt><dd>{{ item.value || '—' }}</dd>
              </template>
            </dl>
            <form class="panel-form" @submit.prevent="update({ name: attribute, value })">
              <label>Delivery attribute<AppSelect :model-value="attribute" :options="availableAttributes" :disabled="busy || !!detailError" placeholder="Choose an attribute" @update:model-value="chooseAttribute" /></label>
              <label v-if="attribute">Value<textarea v-model="value" rows="4" spellcheck="false" :disabled="busy || !!detailError" /></label>
              <button class="primary" :disabled="busy || !!detailError || !attribute || (attribute === 'DeliveryPolicy' && !validPolicy(value))">Save delivery attribute</button>
            </form>
            <section class="panel-form">
              <h3>Bucket notifications</h3>
              <label>Bucket<AppSelect v-model="bucket" :options="bucketOptions" :disabled="bucketLoading" placeholder="Choose a bucket" /></label>
              <Notice v-if="bucketError" tone="error">{{ bucketError }}</Notice>
              <button :disabled="!bucket" @click="emit('notifications', bucket, selected.arn)">Open bucket notifications <ArrowUpRight :size="15" /></button>
            </section>
          </template>
          <form v-else class="panel-form" @submit.prevent="update({ name: 'Policy', value: policy })">
            <label>Topic policy JSON<textarea v-model="policy" rows="12" spellcheck="false" required :disabled="busy || !!detailError" /></label>
            <button class="primary" :disabled="busy || !!detailError || !validPolicy(policy)">Save topic policy</button>
          </form>
        </div>
      </FadeRegion>
    </template>
    <Modal v-if="creating" title="Create SNS topic" @close="!busy && (creating = false)">
      <form class="modal-form" @submit.prevent="create">
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <label>Topic name<input v-model="name" required :disabled="busy" /></label>
        <label v-if="profile.provider === 'ceph'">Push endpoint (optional)<input v-model="endpoint" type="url" :disabled="busy" autocomplete="off" /></label>
        <div class="modal-actions">
          <button type="button" :disabled="busy" @click="creating = false">Cancel</button>
          <button class="primary" :disabled="busy || !name.trim()">Create topic</button>
        </div>
      </form>
    </Modal>
    <ConfirmDialog v-if="removing && selected" :title="`Delete ${selected.name}?`"
      :description="`This deletes ${selected.arn} from the provider.`" action="Delete topic" danger @close="removing = false" @confirm="remove" />
  </div>
</template>

<style scoped>
.settings-description p, .stage-heading p { overflow-wrap: anywhere; }
.topic-attributes { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 2fr); gap: 8px 12px; margin: 0; }
.topic-attributes :is(dt, dd) { overflow-wrap: anywhere; }
.topic-attributes dd { margin: 0; }
</style>
