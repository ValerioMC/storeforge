<script setup lang="ts">
import { computed, onBeforeUnmount, ref, useId } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { TransferAccess, TransferOperation } from '../model/access'

const props = defineProps<{ operation: TransferOperation; target: string; perform: (access: TransferAccess) => void }>()
const emit = defineEmits<{ close: [] }>()
type EncryptionChoice = 'default' | 'customer' | 's3' | 'kms' | 'dualLayerKms'
const encryption = ref<EncryptionChoice>('default')
const keyInput = ref<HTMLInputElement>()
const roleArn = ref('')
const keyId = ref('')
const failure = ref('')
const id = useId()
const title = computed(() => ({ upload: 'Upload with options', download: 'Download with options', read: 'Read with options' })[props.operation])
const modes = computed(() => [
  { value: 'default' as const, label: props.operation === 'upload' ? 'Bucket default' : 'No customer key' },
  { value: 'customer' as const, label: 'Customer-provided key (SSE-C)' },
  ...(props.operation === 'upload' ? [
    { value: 's3' as const, label: 'S3 managed key (SSE-S3)' },
    { value: 'kms' as const, label: 'KMS key (SSE-KMS)' },
    { value: 'dualLayerKms' as const, label: 'Dual-layer KMS (DSSE-KMS)' },
  ] : []),
])

onBeforeUnmount(() => {
  if (keyInput.value) keyInput.value.value = ''
})

function submit(): void {
  const access: TransferAccess = {}
  if (encryption.value === 'customer' && !readCustomerKey(access)) return
  if (encryption.value === 's3') access.encryption = { kind: 's3' }
  if (encryption.value === 'kms' || encryption.value === 'dualLayerKms') {
    access.encryption = { kind: encryption.value, keyId: keyId.value.trim() || undefined }
  }
  if (roleArn.value.trim()) access.roleArn = roleArn.value.trim()
  props.perform(access)
}

function readCustomerKey(access: TransferAccess): boolean {
  const field = keyInput.value
  if (!field) return false
  const value = field.value.trim()
  if (!/^[A-Za-z0-9+/]{43}=$/.test(value)) {
    failure.value = 'Enter a base64-encoded 32-byte customer key.'
    field.focus()
    return false
  }
  access.customerKey = value
  field.value = ''
  return true
}
</script>

<template>
  <Modal :title="title" @close="emit('close')">
    <form class="modal-form" @submit.prevent="submit">
      <code class="access-target">{{ target }}</code>
      <label :for="id + '-encryption'">Encryption</label>
      <AppSelect :id="id + '-encryption'" v-model="encryption" :options="modes" />
      <label v-if="encryption === 'customer'">Customer key
        <input ref="keyInput" type="password" autocomplete="off" spellcheck="false" required />
        <small class="muted">Base64-encoded 32-byte key. Used for this operation.</small>
      </label>
      <label v-if="encryption === 'kms' || encryption === 'dualLayerKms'">KMS key identifier
        <input v-model="keyId" placeholder="Endpoint default" autocomplete="off" spellcheck="false" />
        <small class="muted">Use the full key ARN with a temporary role.</small>
      </label>
      <label>Temporary IAM role ARN
        <input v-model="roleArn" placeholder="Use saved connection credentials" autocomplete="off" spellcheck="false" />
      </label>
      <Notice v-if="failure" tone="error">{{ failure }}</Notice>
      <footer class="modal-actions">
        <button type="button" @click="emit('close')">Cancel</button>
        <button class="primary">{{ operation === 'upload' ? 'Choose files' : operation === 'download' ? 'Choose destination' : 'Read object' }}</button>
      </footer>
    </form>
  </Modal>
</template>

<style scoped>
.access-target { overflow-wrap: anywhere; white-space: pre-wrap; }
</style>
