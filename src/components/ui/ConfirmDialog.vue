<script setup lang="ts">
import { computed, ref } from 'vue'
import Modal from './Modal.vue'
const props = defineProps<{ title: string; description: string; action: string; phrase?: string; danger?: boolean }>()
const emit = defineEmits<{ close: []; confirm: [] }>()
const typed = ref('')
const valid = computed(() => props.phrase == null || typed.value === props.phrase)
</script>

<template>
  <Modal :title="title" @close="emit('close')">
    <form class="modal-form" @submit.prevent="valid && emit('confirm')">
      <p class="muted">{{ description }}</p>
      <slot />
      <label v-if="phrase">Type <code>{{ phrase }}</code> to continue
        <input v-model="typed" autocomplete="off" spellcheck="false" />
      </label>
      <div class="modal-actions">
        <button type="button" @click="emit('close')">Cancel</button>
        <button :class="danger ? 'danger' : 'primary'" :disabled="!valid">{{ action }}</button>
      </div>
    </form>
  </Modal>
</template>
