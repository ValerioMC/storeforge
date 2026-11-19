// @vitest-environment jsdom
import { createPinia } from 'pinia'
import { createApp, type App as VueApp } from 'vue'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import App from '../src/App.vue'
import { zeroRates, type CostMeasurement, type CostReport } from '../src/features/monitoring/model/storage-costs'

const shell = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({ desktop: shell.desktop, isTauri: () => false, errorMessage: String }))

let app: VueApp | undefined
let host: HTMLDivElement

beforeEach(() => {
  const values = new Map<string, string>()
  Object.defineProperty(window, 'localStorage', { configurable: true, value: {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value) },
    removeItem: (key: string) => { values.delete(key) },
  } })
  shell.desktop.mockReset()
  host = document.createElement('div')
  document.body.append(host)
})

afterEach(() => {
  app?.unmount()
  app = undefined
  host.remove()
  window.history.replaceState(null, '', '/')
})

function mountApp(): void {
  app = createApp(App).use(createPinia())
  app.mount(host)
}

function button(label: string): HTMLButtonElement {
  const found = [...host.querySelectorAll('button')].find(item => item.textContent?.trim() === label && !item.closest('[inert]'))
  if (!found) throw new Error(`Button ${label} was unavailable`)
  return found
}

function tab(label: string): HTMLButtonElement {
  const found = [...host.querySelectorAll<HTMLButtonElement>('.workspace-tabs button')].find(item => item.textContent?.trim() === label)
  if (!found) throw new Error(`Tab ${label} was unavailable`)
  return found
}

function panelHolding(stack: Element, selector: string): Element | undefined {
  return [...stack.children].find(panel => panel.matches(selector) || panel.querySelector(selector))
}

function costReport(): CostReport {
  const measurement: CostMeasurement = {
    day: '2024-02-29', collectedAt: '2024-02-29T15:00:00Z', endpointId: 'ridge',
    subject: { kind: 'user', identity: { tenant: 'tenant', uid: 'operator' } }, name: 'Operator', accountId: null,
    storage: { coverage: 'complete', value: 1024 ** 3, detail: null },
    traffic: { coverage: 'missing', value: null, detail: null },
  }
  return {
    endpoints: [{ id: 'ridge', name: 'Ridge RGW' }],
    settings: { currency: 'USD', rates: { storage: 2, ingress: 3, egress: 4, requests: 5 }, overrides: { ridge: zeroRates }, collectStorage: true, collectTraffic: true },
    measurements: [measurement],
  }
}

it('swaps the bucket detail panels inside one stack instead of stacking them in the page flow', async () => {
  window.history.replaceState(null, '', '/?demo=1')
  mountApp()
  await vi.waitFor(() => button('Bucket inventory'))
  button('Bucket inventory').click()
  await vi.waitFor(() => expect(host.querySelector('.bucket-record')).not.toBeNull())
  host.querySelector<HTMLButtonElement>('.bucket-record')!.click()
  await vi.waitFor(() => expect(host.querySelector('.workspace-drawer .fade-stack')).not.toBeNull())
  const stack = host.querySelector('.workspace-drawer .fade-stack')!
  const overview = stack.querySelector('.setup-summary')!.parentElement!

  tab('Usage').click()
  await vi.waitFor(() => expect(stack.querySelector('.bucket-usage')).not.toBeNull())
  expect(overview.parentElement).toBe(stack)
  expect(panelHolding(stack, '.bucket-usage')).toBeDefined()

  tab('Configuration').click()
  await vi.waitFor(() => expect(stack.querySelector('.bucket-configuration')).not.toBeNull())
  expect(panelHolding(stack, '.bucket-configuration')).toBeDefined()
  expect(host.querySelectorAll('.workspace-drawer .bucket-configuration')).toHaveLength(1)
})

it('swaps the storage cost settings panel and the cost tabs inside one stack', async () => {
  shell.desktop.mockImplementation((command: string) => Promise.resolve(command === 'storage_cost_report' ? costReport() : []))
  mountApp()
  await vi.waitFor(() => button('Storage costs'))
  button('Storage costs').click()
  await vi.waitFor(() => expect(host.querySelector('.cost-tab-content')).not.toBeNull())
  const stack = host.querySelector('.cost-tab-content')!.parentElement!
  expect(stack.classList.contains('fade-stack')).toBe(true)

  tab('Cost settings').click()
  await vi.waitFor(() => expect(host.querySelector('.cost-tab-content')).toBeNull())
  const settings = panelHolding(stack, 'form.cost-stack') as HTMLElement
  expect(settings).toBeDefined()
  expect(settings.style.display).not.toBe('none')
})
