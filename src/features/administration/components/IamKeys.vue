<script setup lang="ts">
import { onScopeDispose, ref } from 'vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { issueKey, listKeys, writeQuery } from '../api/iam'
import { useQueryPage } from '../composables/useQueryPage'
import { keyAnnotationId, type IamIdentity, type IamKey, type IssuedIamKey } from '../model/iam'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ connection: string; user: IamIdentity; readOnly: boolean }>()
const pending = ref<{ action: 'create' | 'status' | 'delete'; key?: IamKey }>()
const issued = ref<IssuedIamKey>()
const notes = ref<Record<string, string>>({})
const error = ref('')
const busy = ref(false)
const { items, markers, nextMarker, loading, error: listError, next, previous, reload } = useQueryPage(
  () => [props.connection, props.user.name], marker => listKeys(props.connection, props.user.name, marker), key => key.id,
)
let disposed = false
onScopeDispose(() => {
  disposed = true
  issued.value = undefined
})

function note(key: string): string {
  return notes.value[key] ?? window.localStorage.getItem(keyAnnotationId(props.connection, props.user.id, key)) ?? ''
}

function saveNote(key: string): void {
  try {
    window.localStorage.setItem(keyAnnotationId(props.connection, props.user.id, key), note(key))
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}

async function apply(): Promise<void> {
  const change = pending.value
  if (!change || props.readOnly || busy.value) return
  pending.value = undefined
  busy.value = true
  error.value = ''
  try {
    if (change.action === 'create') {
      const key = await issueKey(props.connection, props.user.name)
      if (!disposed) issued.value = key
    } else if (change.key) {
      const values = { UserName: props.user.name, AccessKeyId: change.key.id }
      if (change.action === 'delete') {
        await writeQuery(props.connection, 'DeleteAccessKey', values)
        window.localStorage.removeItem(keyAnnotationId(props.connection, props.user.id, change.key.id))
      } else {
        await writeQuery(props.connection, 'UpdateAccessKey', { ...values, Status: change.key.status === 'Active' ? 'Inactive' : 'Active' })
      }
    }
  } catch (failure) {
    if (!disposed) error.value = errorMessage(failure)
  } finally {
    if (!disposed) await reload()
    busy.value = false
  }
}
</script>

<template>
  <section class="panel-form">
    <div class="stage-heading"><h3>Access keys</h3><button v-if="!readOnly" :disabled="busy" @click="pending = { action: 'create' }">Create access key</button></div>
    <Notice v-if="error || listError" tone="error">{{ error || listError }}</Notice>
    <p v-else-if="loading" role="status">Loading access keys…</p>
    <p v-else-if="!items.length" class="muted">No access keys.</p>
    <div v-for="key in items" :key="key.id" class="settings-row">
      <div class="settings-description"><strong>{{ key.id }}</strong><p>{{ key.status }} · {{ key.created }}</p></div>
      <label>Local note <input :value="note(key.id)" :disabled="busy" @input="notes[key.id] = ($event.target as HTMLInputElement).value" @change="saveNote(key.id)" /></label>
      <template v-if="!readOnly">
        <button :disabled="busy" @click="pending = { key, action: 'status' }">{{ key.status === 'Active' ? 'Deactivate' : 'Activate' }}</button>
        <button :disabled="busy" @click="pending = { key, action: 'delete' }">Delete</button>
      </template>
    </div>
    <QueryPager :busy="loading" :previous="!!markers.length" :next="!!nextMarker" @previous="previous" @next="next" />
    <ConfirmDialog v-if="pending" :title="pending.action === 'create' ? 'Create access key?' : pending.action === 'delete' ? 'Delete access key?' : 'Change access key status?'"
      :description="pending.key ? `${pending.key.id} belongs to ${user.name}.` : `The provider will issue a credential for ${user.name}.`"
      action="Apply change" :danger="pending.action === 'delete'" @close="pending = undefined" @confirm="apply" />
    <Modal v-if="issued" title="New access key" @close="issued = undefined">
      <div class="modal-form">
        <p>Copy this credential now. The secret is available until you close this dialog.</p>
        <label>IAM user<input :value="issued.owner" readonly /></label>
        <label>Access key ID<input :value="issued.id" readonly /></label>
        <label>Secret access key<input :value="issued.secret" readonly spellcheck="false" autocomplete="off" /></label>
        <div class="modal-actions"><button class="primary" @click="issued = undefined">Done</button></div>
      </div>
    </Modal>
  </section>
</template>
