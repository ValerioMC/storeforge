<script setup lang="ts">
import { Maximize, Minus, X } from 'lucide-vue-next'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { errorMessage } from '../../common/tauri'

const emit = defineEmits<{ error: [message: string] }>()
const desktopWindow = getCurrentWindow()

async function controlWindow(action: 'minimize' | 'toggleMaximize' | 'close'): Promise<void> {
  try {
    await desktopWindow[action]()
  } catch (failure) {
    emit('error', errorMessage(failure))
  }
}
</script>

<template>
  <div class="window-controls" role="group" aria-label="Window controls">
    <button type="button" title="Minimize" aria-label="Minimize" @click="controlWindow('minimize')"><Minus :size="15" /></button>
    <button type="button" title="Maximize or restore" aria-label="Maximize or restore" @click="controlWindow('toggleMaximize')"><Maximize :size="14" /></button>
    <button type="button" title="Close" aria-label="Close" @click="controlWindow('close')"><X :size="16" /></button>
  </div>
</template>
