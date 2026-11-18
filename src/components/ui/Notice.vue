<script setup lang="ts">
import { CircleAlert, CircleCheck, Info } from 'lucide-vue-next'
withDefaults(defineProps<{ tone?: 'error' | 'success' | 'info' }>(), { tone: 'info' })
</script>

<template>
  <div class="notice" :class="tone" :role="tone === 'error' ? 'alert' : 'status'">
    <CircleAlert v-if="tone === 'error'" :size="16" />
    <CircleCheck v-else-if="tone === 'success'" :size="16" />
    <Info v-else :size="16" />
    <span><slot /></span>
  </div>
</template>
