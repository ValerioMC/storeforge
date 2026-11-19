import type { MetadataUpdate, ObjectDetails, ObjectId, ObjectRow, Tag } from './objects'

export const inspectorTabs = ['Preview', 'Details', 'Versions', 'Tags', 'Access'] as const
export type InspectorTab = typeof inspectorTabs[number]

export interface ObjectView extends ObjectId {
  deleted?: boolean
  initialTab?: InspectorTab
}

export const metadataHeaders = [
  { key: 'contentType', label: 'Content type' },
  { key: 'cacheControl', label: 'Cache control' },
  { key: 'contentDisposition', label: 'Content disposition' },
  { key: 'contentEncoding', label: 'Content encoding' },
  { key: 'contentLanguage', label: 'Content language' },
  { key: 'expires', label: 'Expires' },
] as const

type MetadataHeader = typeof metadataHeaders[number]['key']

export interface MetadataDraft {
  headers: Record<MetadataHeader, string>
  raw: string
  dirty: boolean
}

export function readMetadataDraft(details: ObjectDetails): MetadataDraft {
  return {
    headers: {
      contentType: details.contentType ?? '', cacheControl: details.cacheControl ?? '',
      contentDisposition: details.contentDisposition ?? '', contentEncoding: details.contentEncoding ?? '',
      contentLanguage: details.contentLanguage ?? '', expires: details.expires ?? '',
    },
    raw: JSON.stringify(details.metadata, null, 2),
    dirty: false,
  }
}

export function metadataUpdate(draft: MetadataDraft): MetadataUpdate {
  const parsed: unknown = JSON.parse(draft.raw)
  if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object' || Object.entries(parsed).some(([key, value]) => !key || typeof value !== 'string')) {
    throw new Error('Metadata must be a JSON object with nonempty keys and string values.')
  }
  return {
    contentType: draft.headers.contentType || null, cacheControl: draft.headers.cacheControl || null,
    contentDisposition: draft.headers.contentDisposition || null, contentEncoding: draft.headers.contentEncoding || null,
    contentLanguage: draft.headers.contentLanguage || null, expires: draft.headers.expires || null,
    metadata: parsed as Record<string, string>,
  }
}

export function metadataMatches(expected: MetadataUpdate, actual: ObjectDetails): boolean {
  const actualValues = new Map(Object.entries(actual.metadata).map(([key, value]) => [key.toLowerCase(), value]))
  const valuesMatch = Object.entries(expected.metadata).every(([key, value]) => actualValues.get(key.toLowerCase()) === value)
  return valuesMatch && actualValues.size === Object.keys(expected.metadata).length &&
    metadataHeaders.every(({ key }) => metadataHeaderMatches(key, expected[key], actual[key]))
}

function metadataHeaderMatches(key: MetadataHeader, expected: string | null, actual: string | null): boolean {
  if (key === 'expires' && expected && actual) return Date.parse(expected) === Date.parse(actual)
  if (key === 'contentType' && !expected && actual === 'application/octet-stream') return true
  return (expected ?? '') === (actual ?? '')
}

export function tagsMatch(expected: Tag[], actual: Tag[]): boolean {
  const actualValues = new Map(actual.map(tag => [tag.key, tag.value]))
  return expected.length === actualValues.size && expected.every(tag => actualValues.get(tag.key) === tag.value)
}

export function inspectorObjectKey(connection: string, revision: string | undefined, object: ObjectId): string {
  return JSON.stringify([connection, revision, object.bucket, object.key, object.versionId])
}

export function neighboringRow(rows: ObjectRow[], object: ObjectId, direction: -1 | 1): ObjectRow | undefined {
  const objects = rows.filter(row => !row.folder)
  const index = objects.findIndex(row => row.key === object.key)
  return index >= 0 ? objects[index + direction] : undefined
}
