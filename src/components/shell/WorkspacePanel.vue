<script setup lang="ts">
defineProps<{ title: string; description?: string }>()
</script>

<template>
  <section class="workspace-panel">
    <header class="workspace-panel-header">
      <div><h2>{{ title }}</h2><p v-if="description">{{ description }}</p></div>
      <slot name="actions" />
    </header>
    <div v-if="$slots.chart" class="workspace-panel-chart"><slot name="chart" /></div>
    <slot />
  </section>
</template>
