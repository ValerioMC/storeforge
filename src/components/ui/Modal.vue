<script setup lang="ts">
import { onMounted, ref, useId } from 'vue'
import { X } from 'lucide-vue-next'

defineProps<{ title: string; wide?: boolean }>()
const emit = defineEmits<{ close: [] }>()
const dialog = ref<HTMLDialogElement>()
const titleId = useId()

onMounted(() => dialog.value?.showModal())
</script>

<template>
  <Teleport to="body">
    <dialog ref="dialog" class="modal" :class="{ 'modal-wide': wide }" :aria-labelledby="titleId" @cancel.prevent="emit('close')">
      <header class="modal-header">
        <h2 :id="titleId">{{ title }}</h2>
        <button type="button" class="icon-button subtle" aria-label="Close dialog" @click="emit('close')"><X :size="18" /></button>
      </header>
      <slot />
    </dialog>
  </Teleport>
</template>
