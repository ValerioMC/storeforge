<script setup lang="ts">
import { ref } from 'vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { bucketsApi } from '../api/buckets'

const props = defineProps<{ connection: string }>()
const emit = defineEmits<{ close: []; created: [name: string] }>()
const name = ref('')
const lock = ref(false)
const busy = ref(false)
const error = ref('')

async function create(): Promise<void> {
  if (!name.value.trim() || busy.value) return
  busy.value = true
  error.value = ''
  try {
    await bucketsApi.create(props.connection, name.value.trim(), lock.value)
    emit('created', name.value.trim())
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <Modal title="New bucket" @close="!busy && emit('close')">
    <form class="modal-form" @submit.prevent="create">
      <label>Bucket name<input v-model="name" required minlength="3" maxlength="63" autofocus placeholder="e.g. mission-archive" spellcheck="false" /></label>
      <label class="check-label"><input v-model="lock" type="checkbox" /> Enable Object Lock at creation</label>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <div class="modal-actions"><button type="button" @click="emit('close')">Cancel</button><button class="primary" :disabled="busy || !name.trim()">{{ busy ? 'Creating…' : 'Create bucket' }}</button></div>
    </form>
  </Modal>
</template>
