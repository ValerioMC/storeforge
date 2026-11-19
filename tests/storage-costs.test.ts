// @vitest-environment jsdom
import { createPinia } from 'pinia'
import { createApp, nextTick, type App as VueApp } from 'vue'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import App from '../src/App.vue'
import {
  costCsv, costHistory, costPortfolioRows, costRows, measureSubject, portfolioTotals, sortCostRows,
  topCosts, zeroRates, type CostFilters, type CostMeasurement, type CostReport,
} from '../src/features/monitoring/model/storage-costs'

const shell = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({ desktop: shell.desktop, isTauri: () => false, errorMessage: String }))

const gibibyte = 1024 ** 3
const rates = { storage: 2, ingress: 3, egress: 4, requests: 5 }
const february = { month: '2024-02', kind: 'all', endpoint: '', search: '' } as const satisfies CostFilters

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
})

function entry(index: number, day = '2024-02-29', endpointId = 'ridge'): CostMeasurement {
  return {
    day, collectedAt: `${day}T15:00:00Z`, endpointId,
    subject: { kind: 'user', identity: { tenant: 'tenant', uid: `operator-${index}` } },
    name: `Operator ${String(index).padStart(2, '0')}`, accountId: null,
    storage: { coverage: 'complete', value: index * gibibyte, detail: null },
    traffic: {
      coverage: 'partial', value: { bytesReceived: gibibyte, bytesSent: gibibyte, requests: 1000, successfulRequests: null },
      detail: 'Partial usage log',
    },
  }
}

function report(measurements: CostMeasurement[]): CostReport {
  return {
    endpoints: [{ id: 'ridge', name: 'Ridge RGW' }, { id: 'coast', name: 'Coast RGW' }],
    settings: { currency: 'USD', rates, overrides: { coast: zeroRates }, collectStorage: true, collectTraffic: true },
    measurements,
  }
}

function button(label: string): HTMLButtonElement {
  const found = [...host.querySelectorAll('button')].find(item => item.textContent?.trim() === label && !item.closest('[inert]'))
  if (!found) throw new Error(`Button ${label} was unavailable`)
  return found
}

async function waitForText(value: string): Promise<void> {
  await expect.poll(() => host.textContent).toContain(value)
}

async function select(label: string, choice: string): Promise<void> {
  const opener = [...host.querySelectorAll<HTMLButtonElement>('[role="combobox"]')].find(item => item.getAttribute('aria-label') === label)
  if (!opener) throw new Error(`Select ${label} was unavailable`)
  opener.click()
  await nextTick()
  const option = [...document.querySelectorAll<HTMLButtonElement>('[role="option"]')].find(item => item.textContent?.trim() === choice)
  if (!option) throw new Error(`Option ${choice} was unavailable`)
  option.click()
  await nextTick()
}

it('prices the UTC leap month from measured days and keeps missing values distinct from zero', () => {
  const feb = entry(1)
  const partial = entry(1, '2024-02-01')
  partial.storage = { coverage: 'missing', value: null, detail: null }
  partial.traffic.value = { bytesReceived: 0, bytesSent: null, requests: 0, successfulRequests: null }
  const measurements = [partial, feb, entry(1, '2024-03-01'), entry(1, '2024-01-31')]
  const row = measureSubject(measurements, '2024-02', 'Ridge RGW', rates)
  expect(row).toMatchObject({
    daysInMonth: 29, storageDays: 1, trafficDays: 0, partial: true, total: 14,
    usage: { storage: gibibyte, ingress: gibibyte, egress: gibibyte, requests: 1000 },
  })
  expect(measureSubject(measurements, '2024-02', 'Ridge RGW', null)?.total).toBeNull()
  const series = costHistory(report(measurements), february)
  expect(series).toHaveLength(12)
  expect(series[0]).toMatchObject({ month: '2023-03', total: null })
  expect(series.at(-2)).toMatchObject({ month: '2024-01', total: 14 })
  expect(series.at(-1)).toMatchObject({ month: '2024-02', total: 14, partial: true })
  const zero = costRows(report([entry(0, '2024-02-29', 'coast')]), february)
  expect(zero[0]?.total).toBe(0)
  expect(topCosts(zero).fraction).toBeNull()
})

it('sorts and ranks the full result and avoids counting account members twice in portfolio totals', () => {
  const members = Array.from({ length: 23 }, (_, index) => entry(index))
  members[0]!.accountId = 'RGW123'
  const account: CostMeasurement = {
    ...entry(35), subject: { kind: 'account', id: 'RGW123' }, accountId: 'RGW123', name: '=Ridge account',
  }
  const rows = costRows(report([...members, account]), february)
  const scoped = costPortfolioRows(rows)
  expect(scoped).toHaveLength(23)
  const top = topCosts(scoped)
  expect(top.rows).toHaveLength(10)
  expect(top.rows.map(row => row.name).slice(0, 3)).toEqual(['=Ridge account', 'Operator 22', 'Operator 21'])
  expect(top.fraction).toBe(portfolioTotals(top.rows).total! / portfolioTotals(scoped).total!)
  const csv = costCsv(sortCostRows(rows, 'total', true), '2024-02', 'USD')
  expect(csv.trim().split('\r\n')).toHaveLength(25)
  expect(csv).toContain('"\'=Ridge account"')
  expect(csv).toContain('Operator 00')
})

it('preserves the month, endpoint, subject filter, active tab and page after inspecting a subject', async () => {
  const fixture = report([
    ...Array.from({ length: 23 }, (_, index) => entry(index)),
    entry(90, '2024-02-12', 'coast'),
    entry(12, '2024-01-28'),
  ])
  shell.desktop.mockImplementation((command: string, request?: { day?: string }) => {
    if (command === 'storage_cost_report') return Promise.resolve(fixture)
    if (command === 'storage_cost_collect') return Promise.resolve({
      day: request?.day,
      subjects: 1,
      storageRecords: 1,
      trafficRecords: 0,
      failures: [{ endpointId: 'ridge', subject: fixture.measurements[0]!.subject, source: 'traffic', detail: 'No RGW traffic reading.' }],
    })
    return Promise.resolve([])
  })
  app = createApp(App)
  app.use(createPinia())
  app.mount(host)
  await expect.poll(() => host.querySelector('h1')?.textContent).toBe('Connections')
  button('Storage costs').click()
  await waitForText('Top subjects')

  const monthInput = host.querySelector<HTMLInputElement>('input[type=month]')!
  monthInput.value = '2024-02'
  monthInput.dispatchEvent(new Event('change', { bubbles: true }))
  await expect.poll(() => shell.desktop.mock.calls.some(([command, request]) => command === 'storage_cost_report' && request.month === '2024-02-01')).toBe(true)
  await select('Cost endpoint', 'Ridge RGW')
  await select('Subject type', 'Users')
  const search = host.querySelector<HTMLInputElement>('input[type=search]')!
  const subjectSearch = [...host.querySelectorAll<HTMLInputElement>('input[type=search]')].find(input => input.placeholder === 'Find by name or ID') ?? search
  subjectSearch.value = 'Operator'
  subjectSearch.dispatchEvent(new Event('input', { bubbles: true }))
  button('Subjects').click()
  await waitForText('23 matching subjects')
  button('Next page').click()
  await waitForText('Page 2 of 3')
  button('Export CSV').click()
  await expect.poll(() => shell.desktop.mock.calls.find(([command]) => command === 'storage_cost_export')?.[1]?.csv.split('\r\n').length).toBe(25)

  const row = [...host.querySelectorAll<HTMLButtonElement>('[data-cost-subject]')].find(item => item.textContent?.includes('Operator 12'))!
  row.click()
  await waitForText('Daily measurements')
  expect(host.querySelector('h1')?.textContent).toBe('Operator 12')
  button('Back to costs').click()
  await waitForText('Page 2 of 3')
  expect(button('Subjects').getAttribute('aria-pressed')).toBe('true')
  expect(monthInput.isConnected).toBe(false)
  expect(host.querySelector<HTMLInputElement>('input[type=month]')?.value).toBe('2024-02')
  expect(host.querySelector<HTMLInputElement>('input[placeholder="Find by name or ID"]')?.value).toBe('Operator')
  expect(host.querySelector('[aria-label="Cost endpoint"]')?.textContent).toContain('Ridge RGW')
  expect(host.querySelector('[aria-label="Subject type"]')?.textContent).toContain('Users')
  button('Cost settings').click()
  button('Open collection').click()
  await waitForText('Collect a UTC day')
  await select('Collection endpoint', 'Ridge RGW')
  button('Collect daily costs').click()
  await waitForText('No RGW traffic reading.')
  expect(shell.desktop.mock.calls.find(([command]) => command === 'storage_cost_collect')?.[1]).toMatchObject({ endpoint: 'ridge' })
})
