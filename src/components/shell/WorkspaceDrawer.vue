<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, useId } from 'vue'
import { X } from 'lucide-vue-next'

defineProps<{ title: string }>()
const emit = defineEmits<{ close: [] }>()
const titleId = useId()
const closeButton = ref<HTMLButtonElement>()
let returnFocus: HTMLElement | null = null

onMounted(() => {
  returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
  closeButton.value?.focus()
})
onBeforeUnmount(() => {
  if (document.activeElement?.closest('.workspace-drawer')) restoreFocus()
})

function restoreFocus(): void {
  if (returnFocus?.isConnected) returnFocus.focus()
}

function close(): void {
  restoreFocus()
  emit('close')
}
</script>

<template>
  <aside class="workspace-drawer" :aria-labelledby="titleId" @keydown.esc.stop="close">
    <header class="workspace-drawer-header">
      <h2 :id="titleId">{{ title }}</h2>
      <button ref="closeButton" class="icon-button subtle" type="button" aria-label="Close drawer" @click="close"><X :size="17" /></button>
    </header>
    <slot />
  </aside>
</template>
