import type { Connection } from '../../connection/model/connection'
import type { BucketSample, Distribution, HealthSample, HistorySlot, MonitoringView, Reading, TrafficPeriod, UsageSample } from './monitoring'

const integer = new Intl.NumberFormat()
const percent = new Intl.NumberFormat(undefined, { style: 'percent', maximumFractionDigits: 1 })
const millisecondsPerDay = 86_400_000

export function formatCount(value: number | null | undefined): string {
  return value == null ? '—' : integer.format(value)
}

export function formatFraction(value: number | null | undefined): string {
  return value == null ? '—' : percent.format(value)
}

export function formatLatency(value: number | null | undefined): string {
  return value == null ? '—' : `${formatCount(value)} ms`
}

export function sumKnown(values: (number | null | undefined)[]): number | null {
  const present = values.filter((value): value is number => value != null)
  return present.length ? present.reduce((total, value) => total + value, 0) : null
}

export function latestHealth(view: MonitoringView | undefined): HealthSample | undefined {
  return view?.health[0]
}

export function healthIsStale(sample: HealthSample | undefined, reference: string | undefined): boolean {
  return !!sample && !!reference && Date.parse(reference) - Date.parse(sample.checkedAt) >= millisecondsPerDay
}

export function rankedBuckets(buckets: BucketSample[]): BucketSample[] {
  return [...buckets].sort((left, right) =>
    (right.usage.value?.currentBytes ?? -1) - (left.usage.value?.currentBytes ?? -1) || left.bucket.localeCompare(right.bucket))
}

export function storageTotals(sample: UsageSample | undefined) {
  const buckets = sample?.buckets ?? []
  const empty = sample?.bucketCoverage.value === 0
  const usage = buckets.map(bucket => bucket.usage.value)
  const coverage = sample?.bucketCoverage.value == null ? sample?.bucketCoverage.coverage ?? 'missing'
    : sample.bucketCoverage.coverage === 'partial' || buckets.some(bucket => !['currentOnly', 'complete'].includes(bucket.usage.coverage))
      ? 'partial' : buckets.some(bucket => bucket.usage.coverage === 'currentOnly') ? 'currentOnly' : 'complete'
  return {
    coverage,
    bytes: empty ? 0 : sumKnown(usage.map(value => value?.currentBytes)),
    objects: empty ? 0 : sumKnown(usage.map(value => value?.currentObjects)),
    noncurrentBytes: empty ? 0 : sumKnown(usage.map(value => value?.noncurrentBytes)),
    noncurrentObjects: empty ? 0 : sumKnown(usage.map(value => value?.noncurrentObjects)),
    deleteMarkers: empty ? 0 : sumKnown(usage.map(value => value?.deleteMarkers)),
  }
}

export function mergeDistribution(buckets: BucketSample[], field: 'sizes' | 'ages' | 'types' | 'storageClasses'): Distribution {
  const total: Distribution = Object.create(null)
  for (const bucket of buckets) {
    for (const [key, entry] of Object.entries(bucket.usage.value?.[field] ?? {})) {
      const previous = total[key]
      const bytes = previous
        ? previous.bytes == null || entry.bytes == null ? null : previous.bytes + entry.bytes
        : entry.bytes
      total[key] = { objects: (previous?.objects ?? 0) + entry.objects, bytes }
    }
  }
  return total
}

export function transferBytes(reading: Reading<TrafficPeriod> | undefined | null): number | null {
  const counter = reading?.value?.totals
  return counter?.bytesSent != null && counter.bytesReceived != null ? counter.bytesSent + counter.bytesReceived : null
}

export function quotaFraction(used: number | null | undefined, limit: number | null | undefined): number | null {
  if (used == null || limit == null) return null
  if (limit === 0) return used > 0 ? 1 : 0
  return used / limit
}

export function storageTrend(history: HistorySlot[], buckets: BucketSample[]) {
  const measured = history.slice(0, 31).flatMap(({ sample }) =>
    sample?.currentBytes != null && (sample.coverage === 'complete' || sample.coverage === 'currentOnly') ?
      [{ bytes: sample.currentBytes, timestamp: Date.parse(sample.collectedAt) }] : [])
  const latest = measured[0]
  const earlier = measured.at(-1)
  const comparable = !!latest && !!earlier && latest.timestamp > earlier.timestamp
  const growth = comparable ? latest.bytes - earlier.bytes : null
  const days = comparable ? (latest.timestamp - earlier.timestamp) / millisecondsPerDay : null
  const allQuotasKnown = buckets.length > 0 && buckets.every(bucket => bucket.quota.value?.enabled && bucket.quota.value.maxBytes != null &&
    bucket.rgwStorage.value?.bytes != null && bucket.rgwStorage.value.bytes === bucket.usage.value?.currentBytes)
  const limit = allQuotasKnown ? sumKnown(buckets.map(bucket => bucket.quota.value?.maxBytes)) : null
  const projectedDays = growth != null && growth > 0 && days && limit != null && latest
    ? Math.max(0, Math.ceil((limit - latest.bytes) / (growth / days))) : null
  const projectionDetail = limit == null ? 'Requires complete quota and storage readings'
    : growth == null ? 'Requires two comparable storage readings'
    : growth <= 0 ? 'No positive growth in the sampled period'
    : 'At the observed growth rate'
  const quota = history.slice(0, 31).flatMap(({ sample }) => sample?.quotaUsage == null ? [] :
    [{ value: sample.quotaUsage, sampledAt: sample.collectedAt }])
  const firstQuota = quota[0]
  const lastQuota = quota.at(-1)
  const quotaChange = quota.length > 1 && firstQuota && lastQuota ? (firstQuota.value - lastQuota.value) * 100 : null
  return { growth, days, projectedDays, projectionDetail, quotaUsage: firstQuota?.value, quotaAt: firstQuota?.sampledAt, quotaChange }
}

export interface EndpointReading {
  profile: Connection
  view?: MonitoringView
  failure?: string
}

export function endpointReadings(profiles: Connection[], views: Record<string, MonitoringView>, errors: Record<string, string>): EndpointReading[] {
  const endpoints = new Map<string, EndpointReading>()
  for (const profile of profiles) {
    const endpoint = profile.endpointId ?? profile.id
    const previous = endpoints.get(endpoint)
    const view = views[profile.id]
    const checked = latestHealth(view)?.checkedAt
    const previousCheck = latestHealth(previous?.view)?.checkedAt
    if (!previous || (checked && (!previousCheck || Date.parse(checked) > Date.parse(previousCheck)))) {
      endpoints.set(endpoint, { profile, view, failure: errors[profile.id] })
    }
  }
  return Array.from(endpoints.values())
}

export function supervisedTotals(profiles: Connection[], views: Record<string, MonitoringView>) {
  const ceph = profiles.filter(profile => profile.provider === 'ceph')
  const bucketReadings = new Map<string, BucketSample>()
  const endpointTraffic = new Map<string, { period: TrafficPeriod; stale: boolean }>()
  const expectedEndpoints = new Set(ceph.map(profile => profile.endpointId ?? profile.id))
  let incompleteStorage = false
  let storageStale = false
  for (const profile of ceph) {
    const view = views[profile.id]
    const endpoint = profile.endpointId ?? profile.id
    const usage = view?.usage
    if (!usage || usage.bucketCoverage.coverage !== 'complete') incompleteStorage = true
    if (view?.stale && usage?.buckets.some(bucket => bucket.rgwStorage.value != null)) storageStale = true
    for (const bucket of usage?.buckets ?? []) {
      const key = JSON.stringify([endpoint, view?.scope.tenant, bucket.bucket])
      const previous = bucketReadings.get(key)
      if (!previous || (bucket.rgwStorage.value && (!previous.rgwStorage.value ||
        Date.parse(bucket.collectedAt) > Date.parse(previous.collectedAt)))) bucketReadings.set(key, bucket)
    }
    const traffic = usage?.traffic
    const earlier = endpointTraffic.get(endpoint)
    if (traffic?.coverage === 'complete' && traffic.value && (!earlier ||
      Date.parse(traffic.value.end) > Date.parse(earlier.period.end) ||
      (traffic.value.end === earlier.period.end && earlier.stale && !view?.stale))) {
      endpointTraffic.set(endpoint, { period: traffic.value, stale: view?.stale ?? false })
    }
  }
  const storage = Array.from(bucketReadings.values()).flatMap(bucket => bucket.rgwStorage.value ? [bucket.rgwStorage.value] : [])
  const traffic = Array.from(endpointTraffic.values())
  const latest = traffic.reduce<TrafficPeriod | undefined>((period, item) =>
    !period || Date.parse(item.period.end) > Date.parse(period.end) ? item.period : period, undefined)
  const samePeriod = traffic.filter(({ period }) => period.start === latest?.start && period.end === latest.end)
  return {
    bytes: sumKnown(storage.map(value => value.bytes)),
    objects: sumKnown(storage.map(value => value.objects)),
    bucketsMeasured: storage.length,
    bucketsVisible: bucketReadings.size,
    storagePartial: incompleteStorage || storage.length < bucketReadings.size,
    storageStale,
    requests: sumKnown(samePeriod.map(({ period }) => period.totals.requests)),
    transfer: sumKnown(samePeriod.map(({ period }) => transferBytes({ coverage: 'complete', detail: null, value: period }))),
    trafficInterval: latest && { start: latest.start, end: latest.end },
    trafficStale: samePeriod.some(item => item.stale),
    trafficEndpoints: samePeriod.length,
    expectedEndpoints: expectedEndpoints.size,
  }
}
