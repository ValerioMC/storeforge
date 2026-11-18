<script setup lang="ts">
import type { Component } from 'vue'
defineProps<{ icon: Component; title: string; description?: string }>()
</script>

<template>
  <section class="empty-state">
    <div class="empty-icon"><component :is="icon" :size="25" /></div>
    <h2>{{ title }}</h2>
    <p v-if="description">{{ description }}</p>
    <div v-if="$slots.default" class="empty-actions"><slot /></div>
  </section>
</template>
