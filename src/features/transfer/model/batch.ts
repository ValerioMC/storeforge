import type { MetadataUpdate, ObjectVersion, Tag } from '../../browser/model/objects'

export interface BulkRetention {
  mode: 'GOVERNANCE' | 'COMPLIANCE'
  until: string
  bypassGovernance: boolean
}

export interface BulkAttributes {
  metadata?: MetadataUpdate
  tags?: Tag[]
  storageClass?: string
  acl?: string
  legalHold?: boolean
  retention?: BulkRetention
}

export type VersionRule =
  | { kind: 'restoreDeleted' }
  | { kind: 'restoreAt'; at: string; deleteMissing: boolean }
  | { kind: 'cleanup'; keep: number | null; before: string | null; orphanMarkers: boolean }

export interface VersionPlanItem {
  key: string
  versionId: string
  expectedLatest: string
  action: 'restore' | 'markDeleted' | 'deleteVersion'
}

export const metadataFields = [
  { field: 'contentType', label: 'Content-Type' },
  { field: 'cacheControl', label: 'Cache-Control' },
  { field: 'contentDisposition', label: 'Content-Disposition' },
  { field: 'contentEncoding', label: 'Content-Encoding' },
  { field: 'contentLanguage', label: 'Content-Language' },
  { field: 'expires', label: 'Expires' },
] as const

type MetadataField = (typeof metadataFields)[number]['field']

export interface AttributeDraft {
  metadataEnabled: boolean
  headers: Record<MetadataField, string>
  metadata: string
  tagsEnabled: boolean
  tags: string
  storageClass: string
  acl: string
  legalHold: '' | 'on' | 'off'
  retentionEnabled: boolean
  retentionMode: '' | BulkRetention['mode']
  retainUntil: string
  bypassGovernance: boolean
}

export function emptyAttributeDraft(): AttributeDraft {
  return {
    metadataEnabled: false,
    headers: { contentType: '', cacheControl: '', contentDisposition: '', contentEncoding: '', contentLanguage: '', expires: '' },
    metadata: '', tagsEnabled: false, tags: '', storageClass: '', acl: '', legalHold: '',
    retentionEnabled: false, retentionMode: '', retainUntil: '', bypassGovernance: false,
  }
}

export function literalPairs(value: string): Tag[] {
  const entries: Tag[] = []
  const keys = new Set<string>()
  for (const line of value.split(/\r?\n/)) {
    if (!line.length) continue
    const separator = line.indexOf('=')
    const key = line.slice(0, separator)
    if (separator < 1 || keys.has(key)) throw new Error('Enter one key=value pair per line with distinct, nonempty keys.')
    keys.add(key)
    entries.push({ key, value: line.slice(separator + 1) })
  }
  return entries
}

export function toTimestamp(value: string): string {
  const date = new Date(value)
  if (!Number.isFinite(date.getTime())) throw new Error('Enter a valid date and time.')
  return date.toISOString()
}

export function bulkAttributes(draft: AttributeDraft): BulkAttributes {
  const update: BulkAttributes = {}
  if (draft.metadataEnabled) {
    const { contentType, cacheControl, contentDisposition, contentEncoding, contentLanguage, expires } = draft.headers
    const metadata = literalPairs(draft.metadata)
    update.metadata = {
      contentType: contentType || null, cacheControl: cacheControl || null,
      contentDisposition: contentDisposition || null, contentEncoding: contentEncoding || null, contentLanguage: contentLanguage || null,
      expires: expires ? toTimestamp(expires) : null,
      metadata: Object.fromEntries(metadata.map(tag => [tag.key, tag.value])),
    }
  }
  if (draft.tagsEnabled) {
    update.tags = literalPairs(draft.tags)
    if (update.tags.length > 10) throw new Error('S3 allows up to 10 tags per object.')
  }
  if (draft.storageClass) update.storageClass = draft.storageClass
  if (draft.acl) update.acl = draft.acl
  if (draft.legalHold) update.legalHold = draft.legalHold === 'on'
  if (draft.retentionEnabled) {
    if (!draft.retentionMode) throw new Error('Select a retention mode.')
    const until = toTimestamp(draft.retainUntil)
    if (new Date(until).getTime() <= Date.now()) throw new Error('Choose a future retention date.')
    update.retention = { mode: draft.retentionMode, until, bypassGovernance: draft.bypassGovernance }
  }
  if (!Object.keys(update).length) throw new Error('Choose at least one attribute to update.')
  return update
}

function csvCell(value: string | number | boolean | null): string {
  return '"' + String(value ?? '').replaceAll('"', '""') + '"'
}

export function versionCsv(versions: ObjectVersion[]): string {
  const header = ['Key', 'Version ID', 'Current', 'Delete marker', 'Bytes', 'Last modified', 'Storage class']
  const rows = versions.map(item => [item.key, item.versionId, item.latest, item.deleteMarker, item.size, item.modified, item.storageClass ?? null])
  return [header, ...rows].map(row => row.map(csvCell).join(',')).join('\r\n') + '\r\n'
}
