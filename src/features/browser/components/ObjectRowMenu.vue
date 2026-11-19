<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import type { InspectorTab } from '../model/inspector'
import type { ObjectRow } from '../model/objects'

const props = defineProps<{ row: ObjectRow; x: number; y: number; demo: boolean }>()
const emit = defineEmits<{ close: [restoreFocus: boolean]; open: [tab?: InspectorTab]; download: []; copy: [] }>()
const menu = ref<HTMLElement>()
const placement = computed(() => ({
  left: Math.max(8, Math.min(props.x, window.innerWidth - 190)) + 'px',
  top: Math.max(8, Math.min(props.y, window.innerHeight - (props.row.folder || props.row.deleteMarker ? 90 : 240))) + 'px',
}))

onMounted(() => {
  menu.value?.querySelector('button')?.focus()
  window.addEventListener('pointerdown', clickOutside, true)
})
onUnmounted(() => window.removeEventListener('pointerdown', clickOutside, true))

function clickOutside(event: PointerEvent): void {
  if (event.target instanceof Node && !menu.value?.contains(event.target)) emit('close', false)
}

function moveFocus(event: KeyboardEvent): void {
  const actions = Array.from(menu.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])
  const index = actions.findIndex(item => item === document.activeElement)
  if (event.key === 'Escape' || event.key === 'Tab') {
    event.preventDefault()
    event.stopPropagation()
    emit('close', true)
  } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    const offset = event.key === 'ArrowDown' ? 1 : -1
    actions[(index + offset + actions.length) % actions.length]?.focus()
  }
}
</script>

<template>
  <Teleport to="body">
    <div ref="menu" class="pop-menu row-menu" role="group" aria-label="Object row actions" :style="placement" @keydown="moveFocus">
      <button v-if="row.folder" @click="emit('open')">Open folder</button>
      <button v-if="!row.folder && !row.deleteMarker" @click="emit('open', 'Preview')">Preview object</button>
      <button v-if="!row.folder && !row.deleteMarker" @click="emit('open', 'Details')">Object details</button>
      <button v-if="!row.folder" @click="emit('open', 'Versions')">View versions</button>
      <button v-if="!row.folder && !row.deleteMarker" @click="emit('open', 'Tags')">Object tags</button>
      <button v-if="!row.folder && !row.deleteMarker" @click="emit('open', 'Access')">Object access</button>
      <button v-if="!row.folder && !row.deleteMarker" :disabled="demo" @click="emit('download')">Download object</button>
      <button @click="emit('copy')">{{ row.folder ? 'Copy exact prefix' : 'Copy exact object key' }}</button>
    </div>
  </Teleport>
</template>

<style scoped>
.row-menu { position: fixed; z-index: 30; right: auto; bottom: auto; }
</style>
