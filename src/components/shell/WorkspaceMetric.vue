<script setup lang="ts">
defineProps<{ title: string; value: string | number; detail?: string }>()
defineEmits<{ open: [] }>()
</script>

<template>
  <button class="workspace-metric" type="button" @click="$emit('open')">
    <span>{{ title }}</span>
    <strong>{{ value }}</strong>
    <small v-if="detail">{{ detail }}</small>
  </button>
</template>
