import type { Reading, Traffic } from './monitoring'

export type CostSubject =
  | { kind: 'account'; id: string }
  | { kind: 'user'; identity: { tenant: string; uid: string } }
export type CostComponent = 'storage' | 'ingress' | 'egress' | 'requests'
export type CostRates = Record<CostComponent, number>
export type CostBreakdown = Record<CostComponent, number | null>
export type CostSort = 'name' | 'endpoint' | 'total' | CostComponent
export type CostSubjectFilter = CostSubject['kind'] | 'all'

export interface CostSettings {
  currency: string
  rates: CostRates | null
  overrides: Record<string, CostRates>
  collectStorage: boolean
  collectTraffic: boolean
}

export interface CostMeasurement {
  day: string
  collectedAt: string
  endpointId: string
  subject: CostSubject
  name: string
  accountId: string | null
  storage: Reading<number>
  traffic: Reading<Traffic>
}

export interface CostEndpoint { id: string; name: string }

export interface CostReport {
  settings: CostSettings
  endpoints: CostEndpoint[]
  measurements: CostMeasurement[]
}

export interface ScopedCostSummary {
  currency: string
  rates: CostRates | null
  measurements: CostMeasurement[]
}

export interface CostFailure {
  endpointId: string
  subject: CostSubject | null
  source: 'endpoint' | 'storage' | 'traffic'
  detail: string
}

export interface CostCollection {
  day: string
  subjects: number
  storageRecords: number
  trafficRecords: number
  failures: CostFailure[]
}

export interface CostFilters {
  month: string
  endpoint: string
  kind: CostSubjectFilter
  search: string
}

export interface CostTotals {
  usage: CostBreakdown
  breakdown: CostBreakdown
  total: number | null
}

export interface CostRow extends CostTotals {
  key: string
  endpointId: string
  endpoint: string
  subject: CostSubject
  accountId: string | null
  name: string
  storageDays: number
  trafficDays: number
  daysInMonth: number
  partial: boolean
}

export interface CostSeriesPoint {
  month: string
  total: number | null
  partial: boolean
}

export const costComponents = [
  { key: 'storage', label: 'Storage', unit: 'per GiB-month' },
  { key: 'ingress', label: 'Ingress', unit: 'per GiB' },
  { key: 'egress', label: 'Egress', unit: 'per GiB' },
  { key: 'requests', label: 'Requests', unit: 'per 1,000 requests' },
] as const

export const zeroRates: CostRates = { storage: 0, ingress: 0, egress: 0, requests: 0 }

const gibibyte = 1024 ** 3

export function utcDay(now = new Date()): string {
  return now.toISOString().slice(0, 10)
}

export function validCostMonth(month: string): boolean {
  return /^\d{4}-(0[1-9]|1[0-2])$/.test(month) && !month.startsWith('0000')
}

export function shiftCostMonth(month: string, offset: number): string {
  const date = new Date(`${month}-01T00:00:00Z`)
  date.setUTCMonth(date.getUTCMonth() + offset)
  return `${String(date.getUTCFullYear()).padStart(4, '0')}-${String(date.getUTCMonth() + 1).padStart(2, '0')}`
}

function monthDays(month: string): number {
  const date = new Date(`${month}-01T00:00:00Z`)
  date.setUTCMonth(date.getUTCMonth() + 1, 0)
  return date.getUTCDate()
}

export function subjectLabel(subject: CostSubject): string {
  if (subject.kind === 'account') return subject.id
  return [subject.identity.tenant, subject.identity.uid].filter(Boolean).join('$')
}

export function measurementKey(measurement: CostMeasurement): string {
  return JSON.stringify([measurement.endpointId, measurement.subject.kind, subjectLabel(measurement.subject)])
}

function sumKnown(values: readonly (number | null | undefined)[]): number | null {
  const measured = values.filter((value): value is number => value != null)
  return measured.length ? measured.reduce((total, value) => total + value, 0) : null
}

function componentValues(read: (component: CostComponent) => number | null): CostBreakdown {
  return {
    storage: read('storage'),
    ingress: read('ingress'),
    egress: read('egress'),
    requests: read('requests'),
  }
}

function totals(usage: CostBreakdown, breakdown: CostBreakdown): CostTotals {
  return { usage, breakdown, total: sumKnown(Object.values(breakdown)) }
}

function priced(value: number | null, rate: number | undefined, unit: number): number | null {
  return value == null || rate == null ? null : value / unit * rate
}

export function measureSubject(rows: CostMeasurement[], month: string, endpoint: string, rates: CostRates | null): CostRow | undefined {
  const ordered = rows.filter(row => row.day.startsWith(`${month}-`)).sort((left, right) => right.day.localeCompare(left.day))
  const latest = ordered[0]
  if (!latest) return undefined
  const storage = ordered.flatMap(row => row.storage.value == null ? [] : [row.storage.value])
  const storageTotal = sumKnown(storage)
  const storageDays = ordered.filter(row => row.storage.coverage === 'complete').length
  const trafficDays = ordered.filter(row => row.traffic.coverage === 'complete').length
  const daysInMonth = monthDays(month)
  const usage = {
    storage: storageTotal == null ? null : storageTotal / storage.length,
    ingress: sumKnown(ordered.map(row => row.traffic.value?.bytesReceived)),
    egress: sumKnown(ordered.map(row => row.traffic.value?.bytesSent)),
    requests: sumKnown(ordered.map(row => row.traffic.value?.requests)),
  }
  return {
    ...totals(usage, componentValues(component => priced(usage[component], rates?.[component], component === 'requests' ? 1000 : gibibyte))),
    key: measurementKey(latest),
    endpointId: latest.endpointId,
    endpoint,
    subject: latest.subject,
    accountId: latest.accountId,
    name: latest.name,
    storageDays,
    trafficDays,
    daysInMonth,
    partial: storageDays < daysInMonth || trafficDays < daysInMonth,
  }
}

export function costRows(report: CostReport, filters: CostFilters): CostRow[] {
  const endpoints = new Map(report.endpoints.map(endpoint => [endpoint.id, endpoint.name]))
  const grouped = new Map<string, CostMeasurement[]>()
  for (const row of report.measurements) {
    if (!row.day.startsWith(`${filters.month}-`) ||
        filters.endpoint && row.endpointId !== filters.endpoint ||
        filters.kind !== 'all' && row.subject.kind !== filters.kind) continue
    const key = measurementKey(row)
    const entries = grouped.get(key)
    if (entries) entries.push(row)
    else grouped.set(key, [row])
  }

  const query = filters.search.trim().toLocaleLowerCase()
  return Array.from(grouped.values()).flatMap(entries => {
    const endpointId = entries[0].endpointId
    const rates = report.settings.overrides[endpointId] ?? report.settings.rates
    const row = measureSubject(entries, filters.month, endpoints.get(endpointId) ?? endpointId, rates)
    return row && [row.name, subjectLabel(row.subject)].some(value => value.toLocaleLowerCase().includes(query)) ? [row] : []
  })
}

export function costPortfolioRows(rows: CostRow[]): CostRow[] {
  const accounts = new Set(rows.flatMap(row => row.subject.kind === 'account' ? [JSON.stringify([row.endpointId, row.subject.id])] : []))
  return rows.filter(row => row.subject.kind === 'account' || !accounts.has(JSON.stringify([row.endpointId, row.accountId])))
}

export function portfolioTotals(rows: CostRow[]): CostTotals {
  return totals(
    componentValues(component => sumKnown(rows.map(row => row.usage[component]))),
    componentValues(component => sumKnown(rows.map(row => row.breakdown[component]))),
  )
}

export function sortCostRows(rows: CostRow[], field: CostSort, descending: boolean): CostRow[] {
  return [...rows].sort((left, right) => compareCostRows(left, right, field, descending) || left.key.localeCompare(right.key))
}

function compareCostRows(left: CostRow, right: CostRow, field: CostSort, descending: boolean): number {
  const direction = descending ? -1 : 1
  if (field === 'name' || field === 'endpoint') return direction * left[field].localeCompare(right[field])
  const a = field === 'total' ? left.total : left.usage[field]
  const b = field === 'total' ? right.total : right.usage[field]
  if (a == null || b == null) return Number(a == null) - Number(b == null)
  return direction * (a - b)
}

export function topCosts(rows: CostRow[]): { rows: CostRow[]; fraction: number | null } {
  const ranked = sortCostRows(rows.filter(row => row.total != null), 'total', true)
  const leading = ranked.slice(0, 10)
  const total = portfolioTotals(rows).total
  const topTotal = portfolioTotals(leading).total
  return { rows: leading, fraction: total && topTotal != null ? topTotal / total : null }
}

export function costHistory(report: CostReport, filters: CostFilters, subjectKey?: string): CostSeriesPoint[] {
  return Array.from({ length: 12 }, (_, index) => {
    const month = shiftCostMonth(filters.month, index - 11)
    const rows = costRows(report, { ...filters, month })
    const scoped = subjectKey ? rows.filter(row => row.key === subjectKey) : costPortfolioRows(rows)
    return { month, total: portfolioTotals(scoped).total, partial: scoped.some(row => row.partial) }
  })
}

export function formatCost(value: number | null | undefined, currency: string): string {
  if (value == null) return '—'
  const minimumFractionDigits = Math.abs(value) > 0 && Math.abs(value) < 0.01 ? 4 : undefined
  return new Intl.NumberFormat(undefined, { style: 'currency', currency, minimumFractionDigits }).format(value)
}

function csvCell(value: string | number | null): string {
  if (value == null) return ''
  const encoded = typeof value === 'string' && /^[\s]*[=+\-@\t\r\n]/.test(value) ? `'${value}` : String(value)
  return `"${encoded.replaceAll('"', '""')}"`
}

export function costCsv(rows: CostRow[], month: string, currency: string): string {
  const header = [
    'Month (UTC)', 'Endpoint', 'Subject type', 'Subject ID', 'Name', 'Currency',
    'Average storage bytes', 'Ingress bytes', 'Egress bytes', 'Requests',
    'Storage cost', 'Ingress cost', 'Egress cost', 'Request cost', 'Total estimate',
    'Complete storage days', 'Complete traffic days', 'Days in month',
  ]
  const lines = rows.map(row => [
    month, row.endpoint, row.subject.kind, subjectLabel(row.subject), row.name, currency,
    row.usage.storage, row.usage.ingress, row.usage.egress, row.usage.requests,
    row.breakdown.storage, row.breakdown.ingress, row.breakdown.egress, row.breakdown.requests, row.total,
    row.storageDays, row.trafficDays, row.daysInMonth,
  ].map(csvCell).join(','))
  return [header.join(','), ...lines].join('\r\n') + '\r\n'
}
