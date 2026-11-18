// @vitest-environment jsdom

import { afterEach, expect, it, vi } from 'vitest'
import { createApp, h, nextTick, ref, type App } from 'vue'
import FadeRegion from '../src/components/ui/FadeRegion.vue'

let app: App | undefined
let host: HTMLDivElement | undefined
let styles: HTMLStyleElement | undefined

function mountRegion() {
  vi.useFakeTimers()
  const active = ref('Objects')
  const afterLeave = vi.fn()
  styles = document.createElement('style')
  styles.textContent = '.region-fade-leave-active { transition-property: opacity; transition-duration: 0.14s; transition-delay: 0s; }'
  document.head.append(styles)
  host = document.createElement('div')
  document.body.append(host)
  app = createApp({
    render: () => h('main', [
      h('nav', 'Workspaces'),
      h(FadeRegion, { onAfterLeave: afterLeave }, {
        default: () => h('section', { key: active.value }, active.value),
      }),
    ]),
  })
  app.mount(host)
  return { host, active, afterLeave }
}

afterEach(() => {
  app?.unmount()
  host?.remove()
  styles?.remove()
  vi.useRealTimers()
})

it('fades only the outgoing region and enters the latest selection', async () => {
  const { host, active, afterLeave } = mountRegion()
  const navigation = host.querySelector('nav')
  const outgoing = host.querySelector('section')
  expect(outgoing?.textContent).toBe('Objects')

  active.value = 'Tools'
  await nextTick()
  expect(outgoing?.classList.contains('region-fade-leave-active')).toBe(true)
  expect(outgoing?.hasAttribute('inert')).toBe(true)
  expect(host.querySelector('section')).toBe(outgoing)
  expect(afterLeave).not.toHaveBeenCalled()

  active.value = 'Activity'
  await nextTick()
  await vi.advanceTimersByTimeAsync(200)
  const incoming = host.querySelector('section')
  expect(incoming?.textContent).toBe('Activity')
  expect(incoming?.hasAttribute('inert')).toBe(false)
  expect(afterLeave).toHaveBeenCalledOnce()
  expect(host.querySelector('nav')).toBe(navigation)
  expect(navigation?.className).toBe('')
})
