<script setup lang="ts">
import { computed, ref } from 'vue'
import { Plus, Trash2 } from 'lucide-vue-next'
import AppSelect from '../../../components/ui/AppSelect.vue'
import { jsonDefaults, newStatement, policyFields, policyLabel } from '../model/policy-document'
import type { JsonObject, JsonType, JsonValue } from '../model/policy-document'

const props = defineProps<{
  value: JsonValue
  field?: string
  scope?: 'policy' | 'statement'
  disabled: boolean
}>()
const emit = defineEmits<{ change: [value: JsonValue] }>()

const newKey = ref('')
const newType = ref<JsonType>('Text')
const typeOptions = (Object.keys(jsonDefaults) as JsonType[]).map(value => ({ value, label: value }))
const effectOptions = computed(() => {
  const effects = ['Allow', 'Deny'].map(value => ({ value, label: value }))
  const value = props.value
  return typeof value === 'string' && value && value !== 'Allow' && value !== 'Deny'
    ? [{ value, label: value }, ...effects]
    : effects
})
const object = computed(() => isObject(props.value) ? props.value : undefined)
const list = computed(() => Array.isArray(props.value) ? props.value : undefined)
const available = computed(() => Object.entries(policyFields(props.scope))
  .filter(([key]) => !Object.hasOwn(object.value ?? {}, key)))
const principal = computed(() => props.field === 'Principal' || props.field === 'NotPrincipal')
const title = computed(() => props.field === undefined ? 'Value' : policyLabel(props.field))
const hasPrincipalValues = computed(() => Object.values(object.value ?? {})
  .some(value => Array.isArray(value) ? value.length > 0 : Boolean(value)))
const objectProperty = computed(() => {
  const key = newKey.value.trim()
  return key && object.value && !Object.hasOwn(object.value, key) ? key : ''
})

function isObject(value: JsonValue): value is JsonObject {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function replaceProperty(key: string, value: JsonValue): void {
  emit('change', { ...object.value, [key]: value })
}

function removeProperty(key: string): void {
  const updated = { ...object.value }
  delete updated[key]
  emit('change', updated)
}

function addProperty(): void {
  if (!objectProperty.value) return
  replaceProperty(objectProperty.value, jsonDefaults[newType.value])
  newKey.value = ''
  newType.value = 'Text'
}

function replaceItem(index: number, value: JsonValue): void {
  emit('change', list.value?.map((item, position) => position === index ? value : item) ?? [])
}

function removeItem(index: number): void {
  emit('change', list.value?.filter((_, position) => position !== index) ?? [])
}

function addItem(): void {
  const added = props.field === 'Statement' ? newStatement() : jsonDefaults[newType.value]
  emit('change', [...(list.value ?? []), added])
}

function updateInput(event: Event): void {
  const input = event.currentTarget as HTMLInputElement
  if (typeof props.value === 'boolean') emit('change', input.checked)
  else if (typeof props.value === 'number') {
    if (Number.isFinite(input.valueAsNumber)) emit('change', input.valueAsNumber)
  } else emit('change', input.value)
}
</script>

<template>
  <component :is="object && field ? 'fieldset' : 'div'" class="policy-fields" :class="{ 'policy-list': object && field }">
    <legend v-if="object && field">{{ title }}</legend>
    <template v-if="object">
      <div v-for="(child, key) in object" :key="key" class="policy-entry">
        <PolicySettingsForm :value="child" :field="key" :scope="key === 'Statement' && isObject(child) ? 'statement' : undefined"
          :disabled="disabled" @change="replaceProperty(key, $event)" />
        <button v-if="!disabled" type="button" class="icon-button subtle"
          :aria-label="`Remove ${policyLabel(key).toLowerCase()}`" @click="removeProperty(key)"><Trash2 :size="15" /></button>
      </div>
      <div v-if="!disabled && available.length" class="panel-actions">
        <button v-for="[key, defaultValue] in available" :key="key" type="button" class="settings-add"
          @click="replaceProperty(key, defaultValue)"><Plus :size="14" /> Add {{ policyLabel(key).toLowerCase() }}</button>
      </div>
      <div v-if="!disabled && !scope" class="policy-add">
        <input v-model="newKey" type="text" aria-label="Field name" placeholder="Field name" spellcheck="false"
          @keydown.enter.prevent="addProperty" />
        <AppSelect v-model="newType" :options="typeOptions" aria-label="Field type" />
        <button type="button" :disabled="!objectProperty" @click="addProperty">Add field</button>
      </div>
      <button v-if="!disabled && scope === 'statement' && field === 'Statement'" type="button" class="settings-add"
        @click="emit('change', [value, newStatement()])"><Plus :size="14" /> Add another statement</button>
      <button v-if="principal && !disabled && !hasPrincipalValues"
        type="button" class="settings-add" @click="emit('change', '')">Use principal text</button>
    </template>
    <fieldset v-else-if="list" class="policy-list">
      <legend>{{ title }}</legend>
      <p v-if="!list.length && disabled" class="muted">None</p>
      <div v-for="(child, index) in list" :key="index" class="policy-entry">
        <PolicySettingsForm :value="child" :field="field === 'Statement' ? `Statement ${index + 1}` : `Value ${index + 1}`"
          :scope="field === 'Statement' ? 'statement' : undefined" :disabled="disabled" @change="replaceItem(index, $event)" />
        <button v-if="!disabled" type="button" class="icon-button subtle"
          :aria-label="field === 'Statement' ? `Remove statement ${index + 1}` : `Remove value ${index + 1}`"
          @click="removeItem(index)"><Trash2 :size="15" /></button>
      </div>
      <div v-if="!disabled" class="panel-actions">
        <AppSelect v-if="field !== 'Statement' && !['Action', 'NotAction', 'Resource', 'NotResource'].includes(field ?? '')"
          v-model="newType" :options="typeOptions" aria-label="Value type" />
        <button type="button" class="settings-add" @click="addItem">
          <Plus :size="14" /> {{ field === 'Statement' ? 'Add statement' : 'Add value' }}
        </button>
      </div>
    </fieldset>
    <label v-else-if="typeof value === 'boolean'" class="check-label">
      <input type="checkbox" :checked="value" :disabled="disabled" @change="updateInput" /> {{ title }}
    </label>
    <label v-else>
      {{ title }}
      <AppSelect v-if="field === 'Effect' && typeof value === 'string'" :model-value="value" :options="effectOptions"
        placeholder="Select an effect" :disabled="disabled" @update:model-value="emit('change', $event)" />
      <input v-else :value="value ?? 'null'" :type="typeof value === 'number' ? 'number' : 'text'"
        :step="typeof value === 'number' ? 'any' : undefined" :disabled="disabled || value === null"
        spellcheck="false" @input="updateInput" />
    </label>
    <button v-if="principal && typeof value === 'string' && !value && !disabled" type="button" class="settings-add"
      @click="emit('change', { AWS: [] })">Add principal fields</button>
  </component>
</template>
