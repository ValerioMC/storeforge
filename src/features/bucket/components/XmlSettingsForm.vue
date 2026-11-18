<script setup lang="ts">
import { computed } from 'vue'
import { Plus, Trash2 } from 'lucide-vue-next'
import AppSelect from '../../../components/ui/AppSelect.vue'
import { addXmlField, setXmlValue, xmlChildren, xmlValue } from '../model/xml-document'
import type { XmlField, XmlValueField } from '../model/xml-settings'

const props = defineProps<{ parent: Element; fields: readonly XmlField[]; revision: number; disabled: boolean }>()
const emit = defineEmits<{ change: [] }>()

const entries = computed(() => props.fields.map(field => ({
  field,
  children: xmlChildren(props.parent, field.name),
  revision: props.revision,
})))

function values(field: XmlField, children: Element[]): (Element | undefined)[] {
  return field.multiple ? children : [children[0]]
}

function selectOptions(field: XmlValueField, child?: Element): { value: string; label: string }[] {
  const current = xmlValue(props.parent, field, child)
  const options = (field.options ?? []).map(value => ({ value, label: value }))
  if (current && !field.options?.includes(current)) options.unshift({ value: current, label: current })
  if (!field.empty) options.unshift({ value: '', label: 'Not set' })
  return options
}

function add(field: XmlField): void {
  addXmlField(props.parent, field)
  emit('change')
}

function remove(child: Element): void {
  child.remove()
  emit('change')
}

function update(field: XmlValueField, value: string, child?: Element): void {
  setXmlValue(props.parent, field, value, child)
  emit('change')
}

function updateInput(field: XmlValueField, event: Event, child?: Element): void {
  const input = event.currentTarget as HTMLInputElement
  update(field, field.kind === 'check' ? String(input.checked) : input.value, child)
}
</script>

<template>
  <div class="settings-fields">
    <div v-for="{ field, children, revision: fieldRevision } in entries" :key="field.name"
      :class="field.kind === 'group' || field.multiple ? 'settings-field-wide' : undefined">
      <template v-if="field.kind === 'group'">
        <fieldset v-for="(child, index) in children" :key="index" class="settings-group" :class="{ 'settings-group-editable': !disabled }">
          <legend>{{ field.label }}{{ field.multiple && children.length > 1 ? ` ${index + 1}` : '' }}</legend>
          <button v-if="!disabled" class="icon-button subtle settings-remove" type="button"
            :aria-label="`Remove ${field.label.toLowerCase()}`" @click="remove(child)"><Trash2 :size="15" /></button>
          <XmlSettingsForm v-if="field.fields.length" :parent="child" :fields="field.fields"
            :revision="fieldRevision" :disabled="disabled" @change="emit('change')" />
          <p v-else class="muted">Enabled</p>
        </fieldset>
      </template>
      <template v-else>
        <div v-for="(child, index) in values(field, children)" :key="index" class="settings-input-row">
          <label v-if="field.kind === 'check'" class="check-label">
            <input type="checkbox" :checked="['true', '1'].includes(xmlValue(parent, field, child))"
              :disabled="disabled" @change="updateInput(field, $event, child)" />
            {{ field.label }}
          </label>
          <label v-else>
            {{ field.label }}
            <AppSelect v-if="field.kind === 'select'" :model-value="xmlValue(parent, field, child)"
              :options="selectOptions(field, child)" :placeholder="field.empty" :disabled="disabled"
              @update:model-value="update(field, $event, child)" />
            <input v-else :type="field.kind" :value="xmlValue(parent, field, child)" :disabled="disabled"
              :min="field.kind === 'number' ? 0 : undefined" spellcheck="false" @input="updateInput(field, $event, child)" />
          </label>
          <button v-if="field.multiple && child && !disabled" class="icon-button subtle" type="button"
            :aria-label="`Remove ${field.label.toLowerCase()}`" @click="remove(child)"><Trash2 :size="15" /></button>
        </div>
      </template>
      <button v-if="!disabled && (field.multiple || (field.kind === 'group' && !children.length))" type="button"
        class="settings-add" @click="add(field)"><Plus :size="14" /> Add {{ field.label.toLowerCase() }}</button>
      <p v-else-if="disabled && !children.length && (field.multiple || field.kind === 'group')" class="muted">
        {{ field.label }} · Not set
      </p>
    </div>
  </div>
</template>
