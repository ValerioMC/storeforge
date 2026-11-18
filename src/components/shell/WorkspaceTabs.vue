<script setup lang="ts" generic="Tab extends string">
defineProps<{ label: string; tabs: readonly { value: Tab; label: string }[] }>()
const active = defineModel<Tab>({ required: true })
</script>

<template>
  <nav class="workspace-tabs" :aria-label="label">
    <button v-for="tab in tabs" :key="tab.value" type="button" :class="{ active: active === tab.value }"
      :aria-pressed="active === tab.value" @click="active = tab.value">{{ tab.label }}</button>
  </nav>
</template>
