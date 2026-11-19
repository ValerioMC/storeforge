export type MaintenanceKind = 'integrity' | 'purge'

export interface BucketScope {
  bucket: string
  prefix: string
}

export interface BucketSelection {
  connection: string
  parallelism: number
  targets: BucketScope[]
}

export type IntegrityMethod = { kind: 'head' | 'full' } | { kind: 'sample'; bytes: number }

export interface IntegrityRequest {
  selection: BucketSelection
  method: IntegrityMethod
  since: string | null
  includeVersions: boolean
}

export interface PurgeRequest {
  selection: BucketSelection
  includeVersions: boolean
  removeBucket: boolean
}

export interface BucketCounts {
  current: number
  history: number
  markers: number
}

export interface PurgeTarget {
  scope: BucketScope
  counts: BucketCounts | null
  historyError: string | null
  error: string | null
}

export interface PurgePreview {
  request: PurgeRequest
  revision: string | null
  buckets: PurgeTarget[]
  confirmation: string
}

export type BucketStatus = 'pending' | 'listing' | 'running' | 'completed' | 'failed' | 'cancelled'
export type FailureStage = 'listing' | 'review' | 'filter' | 'head' | 'get' | 'delete' | 'deleteBucket'

export interface MaintenanceFailure {
  stage: FailureStage
  key: string | null
  version: string | null
  message: string
}

export interface BucketResult {
  scope: BucketScope
  status: BucketStatus
  planned: BucketCounts
  completed: BucketCounts
  failed: number
  failures: MaintenanceFailure[]
  removed: boolean
}

export type MaintenanceReport = {
  connection: string
  buckets: BucketResult[]
  parallelism: number
  includeVersions: boolean
} & (
  | { kind: 'integrity'; method: IntegrityMethod; since: string | null }
  | { kind: 'purge'; removeBucket: boolean }
)

export function scopeLabel(scope: BucketScope): string {
  return scope.prefix ? `${scope.bucket}/${scope.prefix}` : scope.bucket
}
