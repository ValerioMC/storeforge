<script setup lang="ts">
const emit = defineEmits<{ afterLeave: [] }>()

function makeInactive(element: Element): void {
  element.setAttribute('inert', '')
}

function makeActive(element: Element): void {
  element.removeAttribute('inert')
}
</script>

<template>
  <Transition name="region-fade" mode="out-in" @before-enter="makeActive" @before-leave="makeInactive" @leave-cancelled="makeActive" @after-leave="emit('afterLeave')">
    <slot />
  </Transition>
</template>
