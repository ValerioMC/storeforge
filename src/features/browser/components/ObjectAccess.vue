<script setup lang="ts">
import { ref, watch } from 'vue'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import { objectsApi } from '../api/objects'
import type { ObjectChange, ObjectDetails, ObjectId } from '../model/objects'

const props = defineProps<{ connection: string; object: ObjectId; details?: ObjectDetails; demo: boolean }>()
const emit = defineEmits<{ changed: [] }>()
const acl = ref('private')
const mode = ref<'GOVERNANCE' | 'COMPLIANCE'>('GOVERNANCE')
const until = ref('')
const days = ref(1)
const tier = ref('Standard')
const grants = ref<string[]>()
const pending = ref<{ title: string; change: ObjectChange; description: string }>()
const error = ref('')
const result = ref('')
let request = 0

watch(() => [props.connection, props.object.key, props.object.versionId], () => void load(), { immediate: true })

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
  const action = pending.value
  if (!action) return
  pending.value = undefined
  error.value = ''
  try {
    await objectsApi.change(props.connection, props.object, action.change)
    result.value = 'The endpoint accepted the change.'
    emit('changed')
    await load()
  } catch (failure) {
    error.value = errorMessage(failure)
  }
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
      <label>Canned ACL<AppSelect v-model="acl" :disabled="demo"><option value="private">Private</option><option value="bucket-owner-full-control">Bucket owner full control</option><option value="bucket-owner-read">Bucket owner read</option><option value="authenticated-read">Authenticated read</option><option value="public-read">Public read</option></AppSelect></label>
      <button :disabled="demo" @click="pending = { title: 'Replace object ACL?', description: 'This replaces the current grants with the selected access policy.', change: { kind: 'setAcl', acl } }">Update ACL</button>
    </section>
    <section class="panel-form">
      <h3>Object Lock</h3>
      <p class="muted">Legal hold {{ details?.legalHold || 'unknown' }} · {{ details?.retentionMode || 'No retention reported' }}</p>
      <button :disabled="demo" @click="pending = { title: 'Change legal hold?', description: 'This updates the legal hold on the selected object version.', change: { kind: 'legalHold', enabled: details?.legalHold !== 'ON' } }">
        {{ details?.legalHold === 'ON' ? 'Remove legal hold' : 'Enable legal hold' }}
      </button>
      <label>Retention<AppSelect v-model="mode" :disabled="demo"><option>GOVERNANCE</option><option>COMPLIANCE</option></AppSelect></label>
      <label>Retain until (local time)<input v-model="until" type="datetime-local" :disabled="demo" /></label>
      <button :disabled="demo || !until" @click="reviewRetention">Set retention</button>
    </section>
    <section class="panel-form">
      <h3>Archive restore</h3>
      <label>Days available<input v-model.number="days" type="number" min="1" :disabled="demo" /></label>
      <label>Retrieval tier<AppSelect v-model="tier" :disabled="demo"><option>Standard</option><option>Bulk</option><option>Expedited</option></AppSelect></label>
      <button :disabled="demo || days < 1" @click="pending = { title: 'Request archive restore?', description: 'The endpoint queues retrieval and may charge for the request.', change: { kind: 'restoreArchive', days, tier } }">Request restore</button>
    </section>
    <ConfirmDialog v-if="pending" :title="pending.title" :description="pending.description" action="Apply change" @close="pending = undefined" @confirm="apply" />
  </div>
</template>
