export type Coverage = 'complete' | 'currentOnly' | 'partial' | 'denied' | 'unavailable' | 'missing'
export type Identity = 's3' | 'cephAdmin' | 'supervision'
export type CheckMode = 'http' | 'signed'
export type HealthStatus = 'up' | 'slow' | 'down' | 'denied' | 'unavailable'

export interface Scope {
  connectionId: string
  revision: string | null
  endpointId: string
  tenant: string
  account: string
  region: string
}

export interface Origin {
  source: 's3Listing' | 'rgwUsage' | 'rgwBucket' | 'http' | 'signedS3'
  identity: Identity | null
  principalId: string | null
}

export interface Reading<T> {
  coverage: Coverage
  value: T | null
  detail: string | null
}

export type Distribution = Record<string, { objects: number; bytes: number }>

export interface BucketUsage {
  currentBytes: number | null
  noncurrentBytes: number | null
  currentObjects: number | null
  noncurrentObjects: number | null
  deleteMarkers: number | null
  sizes: Distribution
  ages: Distribution
  types: Distribution
  storageClasses: Distribution
}

export interface BucketSample {
  bucket: string
  collectedAt: string
  usage: Reading<BucketUsage>
  quota: Reading<{ enabled: boolean; maxBytes: number | null; maxObjects: number | null }>
  rgwStorage: Reading<{ bytes: number; objects: number }>
}

export interface TrafficPeriod {
  start: string
  end: string
  totals: {
    bytesSent: number | null
    bytesReceived: number | null
    requests: number | null
    successfulRequests: number | null
  }
}

export interface UsageSample {
  collectedAt: string
  listingOrigin: Origin
  quotaOrigin: Origin
  trafficOrigin: Origin
  bucketCoverage: Reading<number>
  buckets: BucketSample[]
  traffic: Reading<TrafficPeriod>
}

export interface UsagePoint {
  collectedAt: string
  coverage: Coverage
  currentBytes: number | null
  currentObjects: number | null
}

export interface HistorySlot {
  periodStart: string
  sample: UsagePoint | null
}

export interface HealthSample {
  checkedAt: string
  origin: Origin
  mode: CheckMode
  status: HealthStatus
  latencyMs: number | null
  baselineMs: number | null
  httpStatus: number | null
  detail: string | null
}

export interface Incident {
  mode: CheckMode
  openedAt: string
  recoveredAt: string | null
  severity: 'slow' | 'down'
}

export interface MonitoringView {
  scope: Scope
  retrievedAt: string
  stale: boolean
  usage: UsageSample | null
  hourly: HistorySlot[]
  daily: HistorySlot[]
  health: HealthSample[]
  incidents: Incident[]
}

export const coverageLabel: Record<Coverage, string> = {
  complete: 'Complete',
  currentOnly: 'Current objects only',
  partial: 'Partial coverage',
  denied: 'Access denied',
  unavailable: 'Unavailable',
  missing: 'No measurement',
}

export const healthLabel: Record<HealthStatus, string> = {
  up: 'Operational',
  slow: 'Slow',
  down: 'Down',
  denied: 'Access denied',
  unavailable: 'Unavailable',
}
