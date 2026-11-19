<script setup lang="ts">
import { ref } from 'vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { RgwQuota } from '../model/rgw'

const props = defineProps<{ label: string; quota: RgwQuota | null; busy: boolean; error: string }>()
const emit = defineEmits<{ close: []; save: [quota: RgwQuota] }>()
const enabled = ref(props.quota?.enabled ?? false)
const bytes = ref<number | string>(props.quota?.maxBytes ?? '')
const objects = ref<number | string>(props.quota?.maxObjects ?? '')

function save(): void {
  if (props.busy) return
  emit('save', {
    enabled: enabled.value,
    maxBytes: typeof bytes.value === 'number' ? bytes.value : null,
    maxObjects: typeof objects.value === 'number' ? objects.value : null,
  })
}
</script>

<template>
  <Modal :title="`Edit ${label.toLowerCase()}`" @close="!busy && emit('close')">
    <form class="modal-form" @submit.prevent="save">
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <label class="check-label"><input v-model="enabled" type="checkbox" :disabled="busy" /> Enforce quota</label>
      <label>Maximum bytes<input v-model.number="bytes" type="number" min="-1" max="9007199254740991" step="1" :disabled="busy" /></label>
      <label>Maximum objects<input v-model.number="objects" type="number" min="-1" max="9007199254740991" step="1" :disabled="busy" /></label>
      <p class="muted">Enter −1 for unlimited. A blank limit keeps the gateway’s current value.</p>
      <div class="modal-actions"><button type="button" :disabled="busy" @click="emit('close')">Cancel</button><button class="primary" :disabled="busy">Save quota</button></div>
    </form>
  </Modal>
</template>
