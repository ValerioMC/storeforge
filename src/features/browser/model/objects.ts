export interface Location {
  connection: string
  bucket: string
  prefix: string
}

export interface ObjectId {
  bucket: string
  key: string
  versionId: string | null
}

export interface ObjectRow {
  key: string
  folder: boolean
  size: number | null
  modified: string | null
  storageClass: string | null
  etag: string | null
  deleteMarker?: string
}

export interface ObjectPage {
  rows: ObjectRow[]
  cursor: string | null
}

export interface VersionCursor {
  key: string
  version: string | null
}

export interface ObjectVersion {
  key: string
  versionId: string
  latest: boolean
  deleteMarker: boolean
  size: number | null
  modified: string | null
}

export interface VersionPage {
  versions: ObjectVersion[]
  cursor: VersionCursor | null
}

export interface ObjectDetails {
  contentType: string | null
  size: number | null
  etag: string | null
  modified: string | null
  versionId: string | null
  storageClass: string | null
  retainUntil: string | null
  retentionMode: string | null
  legalHold: string | null
  encryption: string | null
  restore: string | null
  cacheControl: string | null
  contentDisposition: string | null
  contentEncoding: string | null
  contentLanguage: string | null
  expires: string | null
  metadata: Record<string, string>
}

export interface Tag {
  key: string
  value: string
}

export interface MetadataUpdate {
  contentType: string | null
  cacheControl: string | null
  metadata: Record<string, string>
}

export type ObjectChange =
  | { kind: 'delete' | 'restoreVersion' }
  | { kind: 'setTags'; tags: Tag[] }
  | { kind: 'setMetadata'; update: MetadataUpdate }
  | { kind: 'setAcl'; acl: string }
  | { kind: 'legalHold'; enabled: boolean }
  | { kind: 'retention'; mode: 'GOVERNANCE' | 'COMPLIANCE'; until: string }
  | { kind: 'restoreArchive'; days: number; tier: string }

export interface Preview {
  contentType: string
  data: string
  truncated: boolean
}

export interface MultipartUpload {
  key: string
  uploadId: string
  initiated: string | null
}

export function objectId(bucket: string, key: string, versionId: string | null = null): ObjectId {
  return { bucket, key, versionId }
}

export function parentPrefix(prefix: string): string {
  return prefix.slice(0, prefix.slice(0, -1).lastIndexOf('/') + 1)
}

export function displayName(key: string, prefix: string, folder = false): string {
  const relative = key.startsWith(prefix) ? key.slice(prefix.length) : key
  if (folder && relative.endsWith('/')) return relative.slice(0, -1) || '/'
  return relative || key
}
