import type { Location, ObjectId, Tag } from '../../browser/model/objects'

import type { BulkAttributes, VersionPlanItem, VersionRule } from './batch'
import type { MaintenanceReport, PurgePreview } from './maintenance'

export type Transfer =
  | { kind: 'upload'; target: Location; paths: string[]; overwrite: boolean }
  | { kind: 'download'; connection: string; objects: ObjectId[]; target: string; directory: boolean }
  | { kind: 'archive'; connection: string; objects: ObjectId[]; target: string; bucket?: string; prefix?: string; folders?: string[] }
  | { kind: 'delete'; connection: string; objects: ObjectId[]; folders?: string[]; bucket?: string }
  | { kind: 'tag'; connection: string; objects: ObjectId[]; tags: Tag[] }
  | { kind: 'copy'; source: Location; objects: ObjectId[]; target: Location; moveSource: boolean; overwrite: boolean; folders?: string[]; renameTo?: string }
  | { kind: 'attributes'; source: Location; objects: ObjectId[]; folders: string[]; update: BulkAttributes }
  | { kind: 'applyVersionPlan'; source: Location; rule: VersionRule; items: VersionPlanItem[] }
  | { kind: 'migrate'; source: Location; target: Location; overwrite: boolean }
  | { kind: 'purge'; preview: PurgePreview; confirmation: string }

export type JobStatus = 'running' | 'completed' | 'failed' | 'cancelled' | 'interrupted'

export interface Failure { item: string; message: string }

export interface Job {
  id: string
  label: string
  status: JobStatus
  startedAt: string
  finishedAt: string | null
  total: number
  succeeded: number
  failed: number
  failures: Failure[]
  successfulItems?: string[]
  bytesProcessed?: number
  currentBytes?: number
  current: string | null
  cancelRequested: boolean
  maintenance?: MaintenanceReport
}

export interface Comparison {
  matching: number
  changed: number
  differentEtag: number
  sourceOnly: number
  targetOnly: number
  differences: { key: string; difference: 'changed' | 'differentEtag' | 'sourceOnly' | 'targetOnly'; sourceBytes: number | null; targetBytes: number | null }[]
}

export interface Usage {
  objects: number
  bytes: number
  versions: number
  deleteMarkers: number
}
