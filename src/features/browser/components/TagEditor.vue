<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Plus, X } from 'lucide-vue-next'
import type { Tag } from '../model/objects'

const props = defineProps<{ tags: Tag[]; disabled?: boolean }>()
const emit = defineEmits<{ save: [tags: Tag[]] }>()
const tags = ref<Tag[]>([])
const valid = computed(() => tags.value.every(tag => tag.key.length > 0) && new Set(tags.value.map(tag => tag.key)).size === tags.value.length)
const changed = computed(() => JSON.stringify(tags.value) !== JSON.stringify(props.tags))

watch(() => props.tags, value => { tags.value = value.map(tag => ({ ...tag })) }, { immediate: true })

function add(): void {
  if (tags.value.length < 10) tags.value.push({ key: '', value: '' })
}

function remove(index: number): void {
  tags.value.splice(index, 1)
}
</script>

<template>
  <form class="panel-form" @submit.prevent="valid && changed && emit('save', tags)">
    <p v-if="tags.length === 0" class="muted">No tags set.</p>
    <div v-for="(tag, index) in tags" :key="index" class="tag-inputs">
      <input v-model="tag.key" :aria-label="`Tag ${index + 1} key`" placeholder="Key" :disabled="disabled" required />
      <input v-model="tag.value" :aria-label="`Tag ${index + 1} value`" placeholder="Value" :disabled="disabled" />
      <button type="button" class="icon-button subtle" :aria-label="`Remove tag ${index + 1}`" :disabled="disabled" @click="remove(index)"><X :size="15" /></button>
    </div>
    <div v-if="!disabled" class="panel-actions">
      <button type="button" :disabled="tags.length >= 10" @click="add"><Plus :size="15" /> Add tag</button>
      <button class="primary" :disabled="!valid || !changed">Save tags</button>
    </div>
  </form>
</template>
