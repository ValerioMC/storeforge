import type { Configuration } from '../../bucket/model/bucket'

export interface BucketPair {
  source: string
  target: string
}

export type ContentMode = 'none' | 'size' | 'etag' | 'checksum'

export interface BucketComparisonRequest {
  pairs: BucketPair[]
  content: ContentMode
  modifiedBefore: string | null
  configuration: Configuration[]
}

export interface ComparisonScope {
  sourceConnection: string
  targetConnection: string
  sourceRevision: string | null
  targetRevision: string | null
}

export interface ObjectSnapshot {
  size: number | null
  etag: string | null
  modified: string | null
}

export type ConfigurationSnapshot =
  | { status: 'configured' | 'error'; value: string }
  | { status: 'unset' }

export interface ObjectDifference {
  key: string
  evidence: 'sourceOnly' | 'targetOnly' | 'differentSize' | 'differentDigest' | 'inconclusive'
  source: ObjectSnapshot | null
  target: ObjectSnapshot | null
  matchingEtag: boolean | null
  issue: string | null
}

export interface ConfigurationDifference {
  configuration: Configuration
  source: ConfigurationSnapshot
  target: ConfigurationSnapshot
  different: boolean
  canApply: boolean
}

export interface BucketContentReport {
  sourceCount: number
  targetCount: number
  matchingSize: number
  matchingDigest: number
  differentSize: number
  differentDigest: number
  inconclusive: number
  sourceOnly: number
  targetOnly: number
  afterCutoff: number
  unknownModified: number
  hidden: number
  details: ObjectDifference[]
}

export interface BucketPairReport {
  pair: BucketPair
  contents: BucketContentReport | null
  contentError: string | null
  configuration: ConfigurationDifference[]
}

export interface BucketComparison {
  scope: ComparisonScope
  reports: BucketPairReport[]
}

export type Remediation =
  | ({ kind: 'object' } & Pick<ObjectDifference, 'key' | 'source' | 'target'>)
  | ({ kind: 'configuration' } & Pick<ConfigurationDifference, 'configuration' | 'source' | 'target'>)
