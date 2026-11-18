<script setup lang="ts" generic="Value extends string | boolean">
import { computed, nextTick, ref, shallowRef, useId, useTemplateRef, watch, type CSSProperties } from 'vue'
import { Check, ChevronDown } from 'lucide-vue-next'

defineOptions({ inheritAttrs: false })
const props = defineProps<{
  id?: string
  options: readonly { value: Value; label: string }[]
  placeholder?: string
  disabled?: boolean
  required?: boolean
  tone?: 'rail'
}>()
const model = defineModel<Value>({ required: true })
const id = useId()
const triggerId = computed(() => props.id ?? id)
const listId = `${id}-options`
const trigger = useTemplateRef<HTMLButtonElement>('trigger')
const list = useTemplateRef<HTMLElement>('list')
const target = shallowRef<HTMLElement>()
const placement = ref<CSSProperties>()
const active = ref(-1)
const unavailable = computed(() => props.disabled || !props.options.length)
const selected = computed(() => props.options.findIndex(option => option.value === model.value))
const displayLabel = computed(() => props.options[selected.value]?.label ?? props.placeholder)
let typeahead = ''
let lastTyped = 0

function positionList(anchor: HTMLElement): CSSProperties {
  const rect = anchor.getBoundingClientRect()
  const width = Math.min(Math.max(rect.width, 176), window.innerWidth - 16)
  const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8))
  const below = window.innerHeight - rect.bottom - 13
  const above = rect.top - 13
  const opensUp = below < Math.min(240, props.options.length * 31 + 10) && above > below
  return {
    left: `${left}px`,
    width: `${width}px`,
    maxHeight: `${Math.min(240, opensUp ? above : below)}px`,
    top: opensUp ? undefined : `${rect.bottom + 5}px`,
    bottom: opensUp ? `${window.innerHeight - rect.top + 5}px` : undefined,
  }
}

function openList(initial: number): void {
  const anchor = trigger.value
  if (!anchor || unavailable.value) return
  anchor.focus({ preventScroll: true })
  placement.value = positionList(anchor)
  target.value = anchor.closest('dialog') ?? document.body
  moveTo(initial)
}

function closeList(): void {
  target.value = undefined
  typeahead = ''
}

function toggleList(): void {
  if (target.value) closeList()
  else openList(Math.max(selected.value, 0))
}

function choose(index: number): void {
  const option = props.options[index]
  if (option) model.value = option.value
  closeList()
  trigger.value?.focus({ preventScroll: true })
}

function optionId(index: number): string {
  return `${listId}-${index}`
}

function moveTo(index: number): void {
  active.value = Math.max(0, Math.min(index, props.options.length - 1))
  void nextTick(() => document.getElementById(optionId(active.value))?.scrollIntoView?.({ block: 'nearest' }))
}

function typeToOption(key: string): void {
  const now = Date.now()
  const character = key.toLocaleLowerCase()
  typeahead = now - lastTyped < 700 ? typeahead + character : character
  lastTyped = now
  const prefix = typeahead === character.repeat(typeahead.length) ? character : typeahead
  const start = prefix.length === 1 ? (active.value + 1) % props.options.length : active.value
  const matches = (option: { label: string }) => option.label.toLocaleLowerCase().startsWith(prefix)
  const index = props.options.findIndex((option, position) => position >= start && matches(option))
  const match = index < 0 ? props.options.findIndex(matches) : index
  if (match >= 0) moveTo(match)
}

function onKeydown(event: KeyboardEvent): void {
  const { key } = event
  if (key === 'Tab') {
    closeList()
    return
  }
  if (event.ctrlKey || event.metaKey || event.altKey) return
  if (key === 'ArrowDown' || key === 'ArrowUp') {
    event.preventDefault()
    const step = key === 'ArrowDown' ? 1 : -1
    if (target.value) moveTo(active.value + step)
    else openList(selected.value < 0 && step < 0 ? props.options.length - 1 : Math.max(selected.value, 0))
  } else if (key === 'Home' || key === 'End') {
    event.preventDefault()
    const index = key === 'Home' ? 0 : props.options.length - 1
    if (target.value) moveTo(index)
    else openList(index)
  } else if (key === 'Escape' && target.value) {
    event.preventDefault()
    event.stopPropagation()
    closeList()
  } else if ((key === 'Enter' || key === ' ') && target.value) {
    event.preventDefault()
    choose(active.value)
  } else if (key.length === 1 && key !== ' ') {
    event.preventDefault()
    if (!target.value) openList(Math.max(selected.value, 0))
    typeToOption(key)
  }
}

function closeOnOutsideEvent(event: Event): void {
  const path = event.composedPath()
  if (trigger.value && path.includes(trigger.value)) return
  if (list.value && path.includes(list.value)) return
  closeList()
}

watch(unavailable, disabled => { if (disabled) closeList() })
watch(target, (container, _, onCleanup) => {
  if (!container) return
  window.addEventListener('pointerdown', closeOnOutsideEvent, true)
  window.addEventListener('focusin', closeOnOutsideEvent, true)
  window.addEventListener('scroll', closeOnOutsideEvent, true)
  window.addEventListener('resize', closeList)
  onCleanup(() => {
    window.removeEventListener('pointerdown', closeOnOutsideEvent, true)
    window.removeEventListener('focusin', closeOnOutsideEvent, true)
    window.removeEventListener('scroll', closeOnOutsideEvent, true)
    window.removeEventListener('resize', closeList)
  })
})
</script>

<template>
  <span class="select-field" :class="{ 'select-rail': tone === 'rail' }">
    <button v-bind="$attrs" :id="triggerId" ref="trigger" type="button" class="select-trigger" role="combobox"
      :disabled="unavailable" :aria-required="required || undefined" :aria-expanded="!!target"
      :aria-controls="target ? listId : undefined" :aria-activedescendant="target ? optionId(active) : undefined"
      @click="toggleList" @keydown="onKeydown">
      <span v-if="$slots.leading" class="select-leading" aria-hidden="true"><slot name="leading" /></span>
      <span class="select-value" :class="{ 'select-placeholder': selected < 0 }">{{ displayLabel }}</span>
      <ChevronDown class="select-chevron" :size="14" aria-hidden="true" />
    </button>
  </span>
  <Teleport v-if="target" :to="target">
    <div :id="listId" ref="list" class="select-list" :class="{ 'select-rail': tone === 'rail' }"
      :style="placement" role="listbox" :aria-labelledby="triggerId" @mousedown.prevent>
      <div v-for="(option, index) in options" :id="optionId(index)" :key="String(option.value)"
        class="select-option" :class="{ 'select-option-active': active === index }" role="option"
        :aria-selected="selected === index" @pointermove="active = index" @click="choose(index)">
        <span>{{ option.label }}</span>
        <Check v-if="selected === index" class="select-check" :size="14" aria-hidden="true" />
      </div>
    </div>
  </Teleport>
</template>
