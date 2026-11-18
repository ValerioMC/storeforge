// @vitest-environment jsdom
import { createPinia } from 'pinia'
import { createApp, h, ref, type App as VueApp } from 'vue'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import App from '../src/App.vue'
import { demoConnection } from '../src/features/browser/model/demo'
import { parseProfile } from '../src/features/connection/model/profile-import'
import MonitoringMetrics from '../src/features/monitoring/components/MonitoringMetrics.vue'
import { endpointReadings, storageTrend, supervisedTotals } from '../src/features/monitoring/model/dashboard'
import { demoMonitoring } from '../src/features/monitoring/model/demo'
import type { MetricPage } from '../src/features/connection/model/workspace'
import type { Reading, UsageSample } from '../src/features/monitoring/model/monitoring'

const shell = vi.hoisted(() => ({ desktop: vi.fn(), tauri: false }))
vi.mock('../src/common/tauri', () => ({
  desktop: shell.desktop, isTauri: () => shell.tauri, errorMessage: String,
}))
let app: VueApp | undefined
let host: HTMLDivElement

beforeEach(() => {
  const values = new Map<string, string>()
  Object.defineProperty(window, 'localStorage', { configurable: true, value: {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value) },
    removeItem: (key: string) => { values.delete(key) },
  } })
  window.history.replaceState(null, '', '/')
  shell.desktop.mockReset()
  shell.tauri = false
  host = document.createElement('div')
  document.body.append(host)
})

afterEach(() => {
  app?.unmount()
  app = undefined
  host.remove()
})

function button(label: string, within: ParentNode = host): HTMLButtonElement | undefined {
  return [...within.querySelectorAll('button')].find(item => item.textContent?.trim() === label)
}

function unavailable<T>(coverage: 'denied' | 'unavailable'): Reading<T> {
  return { coverage, detail: null, value: null }
}

it('renders measured zero and distinct partial, stale, denied and unavailable measurements', async () => {
  const reference = demoMonitoring.usage!
  const empty = reference.buckets.at(-1)!
  const usage: UsageSample = {
    ...reference,
    bucketCoverage: { coverage: 'partial', detail: 'Restricted scope', value: 2 },
    buckets: [
      { ...empty, quota: unavailable('denied') },
      { ...empty, bucket: 'offline', usage: unavailable('unavailable'), quota: unavailable('unavailable') },
    ],
    scopedTraffic: unavailable('denied'),
  }
  const metric = ref<MetricPage>('usage')
  app = createApp({ render: () => h(MonitoringMetrics, {
    modelValue: metric.value,
    'onUpdate:modelValue': (value: MetricPage) => { metric.value = value },
    endpointTraffic: false,
    view: { ...demoMonitoring, usage, stale: true },
  }) })
  app.mount(host)

  expect(host.textContent).toContain('0 B')
  expect(host.textContent).toContain('Partial coverage · Stale')
  expect(host.textContent).toContain('Unavailable')
  expect(host.querySelector('[data-state="denied"]')?.textContent).toContain('Access denied')

  button('Traffic')?.click()
  await vi.waitFor(() => expect(host.textContent).toContain('Access denied · Stale'))
  const traffic = Array.from(host.querySelectorAll('section')).find(panel => panel.querySelector('h2')?.textContent === 'RGW traffic')
  expect(traffic?.querySelector('dd')?.textContent).toBe('—')
})

it('opens a filtered bucket from a measurement, and returns to the keyboard target', async () => {
  window.history.replaceState(null, '', '/?demo=1')
  app = createApp(App).use(createPinia())
  app.mount(host)
  await vi.waitFor(() => expect(button('Usage & metrics')).toBeDefined())
  button('Usage & metrics')!.click()
  await vi.waitFor(() => expect(host.textContent).toContain('Buckets by storage'))
  const source = button('mission-archive')!
  source.focus()
  source.click()
  await vi.waitFor(() => expect(host.querySelector('.workspace-drawer h2')?.textContent).toBe('mission-archive'))
  expect(host.querySelectorAll('.manager-workspace tbody tr')).toHaveLength(1)
  window.dispatchEvent(new KeyboardEvent('keydown', { altKey: true, key: 'ArrowLeft' }))
  await vi.waitFor(() => expect(document.activeElement).toBe(source))
  expect(host.textContent).toContain('Buckets by storage')
  expect(shell.desktop).not.toHaveBeenCalled()
})

it('consolidates endpoint-wide traffic and tenant-qualified storage without counting duplicate profiles', () => {
  const first = { ...demoConnection, id: 'first', endpointId: 'ceph' }
  const second = { ...first, id: 'second' }
  const scope = { ...demoMonitoring.scope, endpointId: 'ceph', tenant: 'research' }
  const views = {
    first: { ...demoMonitoring, stale: true, scope: { ...scope, connectionId: 'first' } },
    second: { ...demoMonitoring, stale: true, scope: { ...scope, connectionId: 'second' } },
  }

  expect(supervisedTotals([first, second], views)).toMatchObject({
    trafficEndpoints: 1, expectedEndpoints: 1, requests: 27296, bucketsMeasured: 6,
    storagePartial: false, storageStale: true, trafficStale: true,
    trafficInterval: { start: '2026-11-17T09:00:00Z', end: '2026-11-18T09:00:00Z' },
  })
  expect(endpointReadings([first, second], views, {})).toHaveLength(1)
  views.second.scope.tenant = 'another-tenant'
  expect(supervisedTotals([first, second], views).bucketsMeasured).toBe(12)
})

it('imports both endpoint coordinates and preserves zero', () => {
  const { name, endpoint, provider, region, pathStyle, knownBuckets } = demoConnection
  const profile = { name, endpoint, provider, region, pathStyle, knownBuckets }
  expect(parseProfile(JSON.stringify({
    ...profile, settings: { coordinates: { latitude: 0, longitude: 0 } },
  })).settings.coordinates).toEqual({ latitude: 0, longitude: 0 })
  expect(() => parseProfile(JSON.stringify({
    ...profile, settings: { coordinates: { latitude: 91, longitude: 0 } },
  }))).toThrow('Invalid endpoint coordinates.')
})

it('explains the missing capacity projection while retaining measured growth', () => {
  const trend = storageTrend(demoMonitoring.daily, [])
  expect(trend.growth).toBeGreaterThan(0)
  expect(trend.projectedDays).toBeNull()
  expect(trend.projectionDetail).toBe('Requires complete quota and storage readings')
})
