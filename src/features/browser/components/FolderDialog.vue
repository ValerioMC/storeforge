<script setup lang="ts">
import { ref } from 'vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { objectsApi } from '../api/objects'
import type { Location } from '../model/objects'

const props = defineProps<{ location: Location }>()
const emit = defineEmits<{ close: []; created: [] }>()
const name = ref('')
const busy = ref(false)
const error = ref('')

async function create(): Promise<void> {
  if (!name.value || busy.value) return
  busy.value = true
  error.value = ''
  try {
    const key = `${props.location.prefix}${name.value}${name.value.endsWith('/') ? '' : '/'}`
    await objectsApi.folder(props.location.connection, props.location.bucket, key)
    emit('created')
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <Modal title="New folder" @close="!busy && emit('close')">
    <form class="modal-form" @submit.prevent="create">
      <p class="muted">Create a folder under <code>{{ location.prefix || 'bucket root' }}</code>.</p>
      <label>Folder name<input v-model="name" autofocus required spellcheck="false" /></label>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <div class="modal-actions"><button type="button" @click="emit('close')">Cancel</button><button class="primary" :disabled="busy || !name">{{ busy ? 'Creating…' : 'Create folder' }}</button></div>
    </form>
  </Modal>
</template>
