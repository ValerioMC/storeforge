<script setup lang="ts">
import { ref, watch } from 'vue'
import { Plus, X } from 'lucide-vue-next'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import { filtersOperators, workbenchAuditPresets, workbenchColumns, type WorkbenchFilter } from '../model/workbench'

const props = defineProps<{ filters: WorkbenchFilter[] }>()
const emit = defineEmits<{ close: []; apply: [filters: WorkbenchFilter[]] }>()
const draft = ref(props.filters.map(filter => ({ ...filter })))
const preset = ref('')
const presets = [{ value: '', label: 'Choose an audit filter' }, ...workbenchAuditPresets.map(({ id, label }) => ({ value: id, label }))]
const fields = workbenchColumns.map(column => ({ value: column.id, label: column.label }))

watch(preset, id => {
  const selected = workbenchAuditPresets.find(item => item.id === id)
  if (selected) draft.value = selected.filters.map(filter => ({ ...filter }))
})

function add(): void {
  draft.value.push({ field: 'lifecycle', operator: 'contains', query: '' })
}

function apply(): void {
  emit('apply', draft.value.filter(filter => filter.operator === 'unavailable' || filter.query.trim()))
}
</script>

<template>
  <Modal title="Advanced bucket filters" wide @close="emit('close')">
    <form class="modal-form" @submit.prevent="apply">
      <label class="audit-filter">Audit filters <AppSelect v-model="preset" :options="presets" aria-label="Audit filters" /></label>
      <p class="muted">Buckets must match every filter. Use bytes for storage size and percentages for quota usage.</p>
      <p v-if="!draft.length">No advanced filters.</p>
      <div v-for="(filter, index) in draft" :key="index" class="filter-rule">
        <AppSelect v-model="filter.field" :options="fields" :aria-label="`Filter ${index + 1} field`" />
        <AppSelect v-model="filter.operator" :options="filtersOperators" :aria-label="`Filter ${index + 1} operator`" />
        <input v-model="filter.query" :disabled="filter.operator === 'unavailable'" :aria-label="`Filter ${index + 1} value`" placeholder="Value" />
        <button class="icon-button subtle" type="button" :aria-label="`Remove filter ${index + 1}`" @click="draft.splice(index, 1)"><X :size="16" /></button>
      </div>
      <div class="panel-actions">
        <button type="button" @click="add"><Plus :size="15" /> Add filter</button>
        <button v-if="draft.length" type="button" @click="draft = []">Clear</button>
        <button type="submit" class="primary">Apply filters</button>
      </div>
    </form>
  </Modal>
</template>

<style scoped>
.audit-filter { display: grid; gap: 5px; }
.filter-rule { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, .8fr) minmax(0, 1fr) auto; align-items: center; gap: 8px; }
@media (max-width: 540px) {
  .filter-rule { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
}
</style>
