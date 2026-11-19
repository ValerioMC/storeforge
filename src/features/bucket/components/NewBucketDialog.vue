<script setup lang="ts">
import { computed, ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { Connection } from '../../connection/model/connection'
import { bucketsApi } from '../api/buckets'

const props = defineProps<{ profile: Connection }>()
const emit = defineEmits<{ close: []; created: [name: string] }>()
const name = ref('')
const location = ref('')
const lock = ref(false)
const versioning = ref(false)
const created = ref('')
const busy = ref(false)
const error = ref('')
const aws = computed(() => props.profile.provider === 'aws')
const customRegion = computed(() => aws.value && !!location.value.trim() && location.value.trim() !== props.profile.region)

function close(): void {
  if (busy.value) return
  if (created.value) emit('created', created.value)
  else emit('close')
}

async function create(): Promise<void> {
  if (!name.value.trim() || busy.value) return
  busy.value = true
  error.value = ''
  try {
    if (created.value) {
      await bucketsApi.finishVersioning(props.profile.id, created.value, location.value.trim())
    } else {
      const target = name.value.trim()
      const result = await bucketsApi.create(props.profile.id, target, {
        objectLock: lock.value, location: location.value.trim(), versioning: versioning.value || lock.value,
      })
      created.value = target
      if (result.versioningError) {
        error.value = result.versioningError
        return
      }
    }
    emit('created', created.value)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <Modal title="New bucket" @close="close">
    <form class="modal-form" @submit.prevent="create">
      <template v-if="!created">
        <label>Bucket name<input v-model="name" :disabled="busy" required minlength="3" maxlength="63" autofocus placeholder="e.g. mission-archive" spellcheck="false" /></label>
        <label>{{ aws ? 'AWS region' : 'Location or placement' }}
          <input v-model="location" :disabled="busy" :placeholder="aws ? profile.region : 'Provider default'" spellcheck="false" />
        </label>
        <p v-if="customRegion" class="muted">Use a connection in {{ location.trim() }} to manage this bucket after creation.</p>
        <label class="check-label"><input v-model="lock" :disabled="busy" type="checkbox" /> Enable Object Lock at creation</label>
        <label class="check-label"><input :checked="versioning || lock" :disabled="busy || lock" type="checkbox" @change="versioning = ($event.target as HTMLInputElement).checked" /> Enable versioning{{ lock ? ' (required by Object Lock)' : '' }}</label>
      </template>
      <Notice v-else tone="info">Bucket {{ created }} exists. StoreForge could not confirm that versioning was enabled.</Notice>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="close">{{ created ? 'Done' : 'Cancel' }}</button>
        <button class="primary" :disabled="busy || !name.trim()">{{ created ? 'Retry versioning' : busy ? 'Creating…' : 'Create bucket' }}</button>
      </div>
    </form>
  </Modal>
</template>
