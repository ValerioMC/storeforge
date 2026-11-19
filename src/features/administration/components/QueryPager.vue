<script setup lang="ts">
defineProps<{ busy: boolean; previous: boolean; next: boolean }>()
const emit = defineEmits<{ previous: []; next: [] }>()
</script>

<template>
  <div v-if="previous || next" class="panel-actions">
    <button type="button" :disabled="busy || !previous" @click="emit('previous')">Previous page</button>
    <button type="button" :disabled="busy || !next" @click="emit('next')">Next page</button>
  </div>
</template>
