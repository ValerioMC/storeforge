import { demoBuckets, demoConnection } from '../../browser/model/demo'
import type { BucketSample, HistorySlot, MonitoringView, UsagePoint } from './monitoring'

const collectedAt = '2026-11-18T09:24:00Z'
const gibibyte = 1024 ** 3
const readings = [
  { bytes: 184.6, objects: 86420, className: 'STANDARD', type: 'application/octet-stream', quota: 256 },
  { bytes: 67.1, objects: 23981, className: 'STANDARD', type: 'application/octet-stream', quota: 128 },
  { bytes: 38.9, objects: 10102, className: 'STANDARD_IA', type: 'text/csv', quota: 128 },
  { bytes: 13.7, objects: 5960, className: 'STANDARD', type: 'application/x-parquet', quota: 32 },
  { bytes: 7.4, objects: 1325, className: 'STANDARD', type: 'application/json', quota: 32 },
  { bytes: 0, objects: 0, className: 'GLACIER', type: 'application/octet-stream', quota: 32 },
]
const buckets: BucketSample[] = demoBuckets.map(({ name: bucket }, index) => {
  const entry = readings[index]!
  const bytes = Math.round(entry.bytes * gibibyte)
  const distribution = { objects: entry.objects, bytes }
  return {
    bucket, collectedAt,
    usage: {
      coverage: 'complete', detail: null,
      value: {
        currentBytes: bytes, currentObjects: entry.objects,
        noncurrentBytes: 0, noncurrentObjects: 0, deleteMarkers: 0,
        types: { [entry.type]: distribution }, sizes: { 'Under 1 MiB': distribution },
        ages: { 'Under 7 days': distribution }, storageClasses: { [entry.className]: distribution },
      },
    },
    quota: { coverage: 'complete', detail: null, value: { enabled: true, maxBytes: entry.quota * gibibyte, maxObjects: null } },
    rgwStorage: { coverage: 'complete', detail: null, value: { bytes, objects: entry.objects } },
  }
})
const currentBytes = buckets.reduce((total, bucket) => total + (bucket.usage.value?.currentBytes ?? 0), 0)
const currentObjects = readings.reduce((total, entry) => total + entry.objects, 0)
const quotaLimit = readings.reduce((total, entry) => total + entry.quota * gibibyte, 0)
const daily: HistorySlot[] = Array.from({ length: 31 }, (_, day) => {
  const periodStart = new Date(Date.UTC(2026, 10, 18 - day)).toISOString()
  const bytes = Math.max(Math.round(currentBytes - Math.sqrt(day) * 13 * gibibyte), 0)
  const sample: UsagePoint | null = day === 7 || day === 14 ? null : {
    collectedAt: day === 0 ? collectedAt : periodStart, coverage: 'complete',
    currentBytes: bytes, quotaUsage: bytes / quotaLimit,
    currentObjects: currentObjects - day * 740,
  }
  return { periodStart, sample }
})
const traffic = {
  coverage: 'complete' as const, detail: null,
  value: {
    start: '2026-11-17T09:00:00Z', end: '2026-11-18T09:00:00Z',
    totals: { bytesSent: 10 * gibibyte, bytesReceived: 6 * gibibyte, requests: 27296, successfulRequests: 27155 },
  },
}
const http = { source: 'http' as const, identity: null, principalId: null }

export const demoMonitoring: MonitoringView = {
  scope: { connectionId: demoConnection.id, revision: null, endpointId: demoConnection.id, tenant: '', account: '', region: demoConnection.region },
  retrievedAt: collectedAt, stale: false,
  usage: {
    collectedAt,
    listingOrigin: { source: 's3Listing', identity: 's3', principalId: demoConnection.id },
    quotaOrigin: { source: 'rgwBucket', identity: 'cephAdmin', principalId: null },
    trafficOrigin: { source: 'rgwUsage', identity: 'supervision', principalId: null },
    bucketCoverage: { coverage: 'complete', value: buckets.length, detail: null },
    buckets, traffic, scopedTraffic: traffic,
  },
  hourly: [{
    periodStart: '2026-11-18T09:00:00Z', sample: { collectedAt, coverage: 'complete', currentBytes, currentObjects, quotaUsage: currentBytes / quotaLimit },
  }],
  daily,
  health: [
    { checkedAt: '2026-11-18T09:22:00Z', mode: 'http', origin: http, status: 'up', latencyMs: 82, baselineMs: 74, httpStatus: 200, detail: null },
    { checkedAt: '2026-11-18T06:11:00Z', mode: 'http', origin: http, status: 'slow', latencyMs: 968, baselineMs: 74, httpStatus: 200, detail: null },
  ],
  incidents: [{ mode: 'http', severity: 'slow', openedAt: '2026-11-18T06:11:00Z', recoveredAt: '2026-11-18T09:22:00Z' }],
}
