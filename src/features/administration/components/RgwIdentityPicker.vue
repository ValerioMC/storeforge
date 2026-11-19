<script setup lang="ts">
import { computed, ref } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { listRgwIdentities } from '../api/rgw'
import { useQueryPage } from '../composables/useQueryPage'
import { rgwTargetKey, rgwTargetLabel, type RgwKind, type RgwTarget } from '../model/rgw'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ connection: string; kind: RgwKind; label: string; disabled: boolean }>()
const emit = defineEmits<{ select: [target: RgwTarget] }>()
const selected = ref('')
const { items, markers, nextMarker, loading, error, search, next, previous, reload } = useQueryPage(
  () => [props.connection, props.kind],
  marker => listRgwIdentities(props.connection, props.kind, false, marker),
  row => rgwTargetLabel(row.target),
  10,
)
const options = computed(() => items.value.map(({ target }) => ({ value: rgwTargetKey(target), label: rgwTargetLabel(target) })))

function choose(): void {
  const row = items.value.find(({ target }) => rgwTargetKey(target) === selected.value)
  if (!row) return
  emit('select', row.target)
  selected.value = ''
}
</script>

<template>
  <fieldset class="panel-form rgw-picker" :disabled="disabled">
    <legend>{{ label }}</legend>
    <div class="form-grid">
      <label>Find {{ kind }}<input v-model="search" type="search" @keydown.enter.prevent="reload" /></label>
      <button type="button" :disabled="loading" @click="reload">Search</button>
      <label>Cluster {{ kind }}<AppSelect v-model="selected" :options="options" :disabled="disabled || loading" /></label>
      <button type="button" :disabled="loading || !options.some(option => option.value === selected)" @click="choose">Select {{ kind }}</button>
    </div>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <QueryPager :busy="loading" :previous="!!markers.length" :next="!!nextMarker" @previous="previous" @next="next" />
  </fieldset>
</template>

<style scoped>
.rgw-picker { min-width: 0; border: 1px solid var(--line); border-radius: 7px; padding: 12px; }
.form-grid { align-items: end; }
</style>
