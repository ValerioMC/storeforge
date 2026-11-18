import type { Location, ObjectId, Tag } from '../../browser/model/objects'

export type Transfer =
  | { kind: 'upload'; target: Location; paths: string[]; overwrite: boolean }
  | { kind: 'download'; connection: string; objects: ObjectId[]; target: string; directory: boolean }
  | { kind: 'archive'; connection: string; objects: ObjectId[]; target: string }
  | { kind: 'delete'; connection: string; objects: ObjectId[] }
  | { kind: 'tag'; connection: string; objects: ObjectId[]; tags: Tag[] }
  | { kind: 'copy'; source: Location; objects: ObjectId[]; target: Location; moveSource: boolean; overwrite: boolean }
  | { kind: 'migrate'; source: Location; target: Location; overwrite: boolean }
  | { kind: 'purge'; source: Location; includeVersions: boolean; removeBucket: boolean }

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
  current: string | null
  cancelRequested: boolean
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

export interface Integrity {
  checked: number
  failed: number
  failures: Failure[]
}
