<script setup lang="ts" generic="Value extends string | boolean">
import { ChevronDown } from 'lucide-vue-next'

defineProps<{ id?: string; disabled?: boolean; required?: boolean }>()
const model = defineModel<Value>({ required: true })
</script>

<template>
  <span class="select-field" :class="{ 'select-field-with-icon': $slots.leading }">
    <span v-if="$slots.leading" class="select-leading" aria-hidden="true"><slot name="leading" /></span>
    <select :id="id" v-model="model" :disabled="disabled" :required="required"><slot /></select>
    <ChevronDown class="select-chevron" :size="14" aria-hidden="true" />
  </span>
</template>
