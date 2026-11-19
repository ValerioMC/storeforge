<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import { objectsApi } from '../api/objects'
import type { ObjectChange, ObjectDetails, ObjectId } from '../model/objects'

const props = defineProps<{ connection: string; object: ObjectId; details?: ObjectDetails; demo: boolean }>()
const emit = defineEmits<{ changed: [details: ObjectDetails | undefined] }>()
const acl = ref('private')
const mode = ref<'GOVERNANCE' | 'COMPLIANCE'>('GOVERNANCE')
const until = ref('')
const days = ref(1)
const tier = ref('Standard')
const grants = ref<string[]>()
const pending = ref<{ title: string; change: ObjectChange; description: string }>()
const error = ref('')
const result = ref('')
const applying = ref(false)
let request = 0
const aclOptions = [
  { value: 'private', label: 'Private' },
  { value: 'bucket-owner-full-control', label: 'Bucket owner full control' },
  { value: 'bucket-owner-read', label: 'Bucket owner read' },
  { value: 'authenticated-read', label: 'Authenticated read' },
  { value: 'public-read', label: 'Public read' },
]
const retentionOptions = (['GOVERNANCE', 'COMPLIANCE'] as const).map(value => ({ value, label: value }))
const retrievalOptions = ['Standard', 'Bulk', 'Expedited'].map(value => ({ value, label: value }))

watch(() => [props.connection, props.object.bucket, props.object.key, props.object.versionId], () => {
  pending.value = undefined
  result.value = ''
  applying.value = false
  void load()
}, { immediate: true })
onUnmounted(() => { request++ })

async function load(): Promise<void> {
  const id = ++request
  grants.value = undefined
  error.value = ''
  try {
    const values = await objectsApi.acl(props.connection, props.object)
    if (id === request) grants.value = values
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  }
}

function reviewRetention(): void {
  if (until.value) {
    pending.value = {
      title: 'Set object retention?',
      description: 'A compliance retention period can prevent deletion or shortening until it expires.',
      change: { kind: 'retention', mode: mode.value, until: new Date(until.value).toISOString() },
    }
  }
}

async function apply(): Promise<void> {
  const action = pending.value?.change
  if (!action || applying.value) return
  pending.value = undefined
  result.value = ''
  error.value = ''
  applying.value = true
  const id = request
  const { connection, object } = props
  try {
    await objectsApi.change(connection, object, action)
    if (action.kind === 'setAcl') {
      const verified = await objectsApi.acl(connection, object)
      if (id !== request) return
      grants.value = verified
      result.value = 'Access grants refreshed from the provider.'
      emit('changed', undefined)
    } else {
      const verified = await objectsApi.details(connection, object)
      if (id !== request) return
      emit('changed', verified)
      result.value = verifyDetails(action, verified)
    }
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request) applying.value = false
  }
}

function verifyDetails(action: ObjectChange, verified: ObjectDetails): string {
  if (action.kind === 'legalHold') {
    if (verified.legalHold !== (action.enabled ? 'ON' : 'OFF')) throw new Error('The provider has not confirmed the legal hold change.')
    return 'Legal hold confirmed by the provider.'
  }
  if (action.kind === 'retention') {
    if (action.mode !== verified.retentionMode || Date.parse(action.until) !== Date.parse(verified.retainUntil ?? '')) {
      throw new Error('The provider has not confirmed the requested retention.')
    }
    return 'Retention confirmed by the provider.'
  }
  return verified.restore ? 'Archive restore status refreshed.' : 'Restore requested. Refresh details to check its status.'
}
</script>

<template>
  <div class="panel-form">
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <Notice v-if="result" tone="success">{{ result }}</Notice>
    <section class="panel-form">
      <h3>Access grants</h3>
      <ul v-if="grants?.length" class="grants"><li v-for="grant in grants" :key="grant"><code>{{ grant }}</code></li></ul>
      <p v-else-if="grants" class="muted">No grants returned.</p>
      <button v-else-if="error" class="subtle" @click="load">Retry access grants</button>
      <label>Canned ACL<AppSelect v-model="acl" :options="aclOptions" :disabled="demo" /></label>
      <button :disabled="demo || applying" @click="pending = { title: 'Replace object ACL?', description: 'This replaces the current grants with the selected access policy.', change: { kind: 'setAcl', acl } }">Update ACL</button>
    </section>
    <section class="panel-form">
      <h3>Object Lock</h3>
      <p class="muted">Legal hold {{ details?.legalHold || 'unknown' }} · {{ details?.retentionMode || 'No retention reported' }}</p>
      <button :disabled="demo || applying" @click="pending = { title: 'Change legal hold?', description: 'This updates the legal hold on the selected object version.', change: { kind: 'legalHold', enabled: details?.legalHold !== 'ON' } }">
        {{ details?.legalHold === 'ON' ? 'Remove legal hold' : 'Enable legal hold' }}
      </button>
      <label>Retention<AppSelect v-model="mode" :options="retentionOptions" :disabled="demo" /></label>
      <label>Retain until (local time)<input v-model="until" type="datetime-local" :disabled="demo" /></label>
      <button :disabled="demo || applying || !until" @click="reviewRetention">Set retention</button>
    </section>
    <section class="panel-form">
      <h3>Archive restore</h3>
      <p class="muted">{{ details?.restore || 'No restore status reported' }}</p>
      <label>Days available<input v-model.number="days" type="number" min="1" :disabled="demo" /></label>
      <label>Retrieval tier<AppSelect v-model="tier" :options="retrievalOptions" :disabled="demo" /></label>
      <button :disabled="demo || applying || !Number.isInteger(days) || days < 1" @click="pending = { title: 'Request archive restore?', description: 'The endpoint queues retrieval and may charge for the request.', change: { kind: 'restoreArchive', days, tier } }">Request restore</button>
    </section>
    <ConfirmDialog v-if="pending" :title="pending.title" :description="pending.description" action="Apply change" @close="pending = undefined" @confirm="apply" />
  </div>
</template>
