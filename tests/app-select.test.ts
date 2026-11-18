// @vitest-environment jsdom

import { afterEach, describe, expect, it } from 'vitest'
import { createApp, h, nextTick, ref, type App } from 'vue'
import AppSelect from '../src/components/ui/AppSelect.vue'

const addressingOptions = [
  { label: 'Path style', value: true },
  { label: 'Virtual hosted', value: false },
]

let app: App | undefined
let host: HTMLElement | undefined

function mountSelect(inDialog = false) {
  const value = ref(true)
  const disabled = ref(false)
  host = document.createElement(inDialog ? 'dialog' : 'div')
  if (inDialog) host.setAttribute('open', '')
  const container = host
  document.body.append(container)
  app = createApp({
    render: () => h('form', [
      h(AppSelect<boolean>, {
        modelValue: value.value,
        'onUpdate:modelValue': (selection: boolean) => { value.value = selection },
        options: addressingOptions,
        disabled: disabled.value,
        'aria-label': 'Addressing',
      }),
      h('input', { 'aria-label': 'Another field' }),
    ]),
  })
  app.mount(container)
  const trigger = container.querySelector<HTMLButtonElement>('[role="combobox"]')
  if (!trigger) throw new Error('Expected a select trigger')
  return { container, trigger, value, disabled }
}

async function press(trigger: HTMLElement, key: string): Promise<void> {
  trigger.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
  await nextTick()
}

afterEach(() => {
  app?.unmount()
  host?.remove()
})

describe('AppSelect', () => {
  it('renders its options in an app-owned list and preserves boolean values', async () => {
    const { container, trigger, value } = mountSelect()
    expect(container.querySelector('select')).toBeNull()
    expect(trigger.type).toBe('button')
    trigger.click()
    await nextTick()
    const list = document.getElementById(trigger.getAttribute('aria-controls') ?? '')
    expect(list?.getAttribute('role')).toBe('listbox')
    expect(list?.querySelector('[aria-selected="true"]')?.textContent?.trim()).toBe('Path style')
    list?.querySelector<HTMLElement>('[aria-selected="false"]')?.click()
    await nextTick()
    expect(value.value).toBe(false)
    expect(trigger.textContent?.trim()).toBe('Virtual hosted')
    expect(trigger.getAttribute('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(trigger)
  })

  it('navigates and confirms by keyboard, and cancels with Escape', async () => {
    const { trigger, value } = mountSelect()
    await press(trigger, 'ArrowDown')
    await press(trigger, 'End')
    const activeOption = document.getElementById(trigger.getAttribute('aria-activedescendant') ?? '')
    expect(activeOption?.textContent?.trim()).toBe('Virtual hosted')
    expect(value.value).toBe(true)
    await press(trigger, 'Escape')
    expect(trigger.getAttribute('aria-expanded')).toBe('false')
    expect(value.value).toBe(true)
    await press(trigger, 'v')
    await press(trigger, 'Enter')
    expect(value.value).toBe(false)
  })

  it('keeps the list inside a dialog and dismisses it on outside press or when disabled', async () => {
    const { container, trigger, disabled } = mountSelect(true)
    trigger.click()
    await nextTick()
    expect(container.querySelector('[role="listbox"]')).not.toBeNull()
    container.querySelector('input')?.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }))
    await nextTick()
    expect(container.querySelector('[role="listbox"]')).toBeNull()
    trigger.click()
    await nextTick()
    disabled.value = true
    await nextTick()
    expect(container.querySelector('[role="listbox"]')).toBeNull()
    expect(trigger.disabled).toBe(true)
  })
})
