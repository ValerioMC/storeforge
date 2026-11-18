import { displayName, type Location, type ObjectRow, type ObjectVersion } from './objects'

export type BrowserProfile = 'root' | 'embedded'
export type SortColumn = 'provider' | 'name' | 'size' | 'modified' | 'storageClass'
export type DetailColumn = 'storageClass' | 'contentType' | 'versionId' | 'etag'
export type ResizableColumn = Exclude<SortColumn, 'provider'> | DetailColumn | 'type'

export interface BrowserSort {
  column: SortColumn
  ascending: boolean
}

export interface ListingOptions {
  scope: 'path' | 'bucket'
  kind: 'all' | 'object' | 'folder'
  exact: boolean
  caseSensitive: boolean
  storageClass: string
  includeDeleted: boolean
  limit: number
}

export function defaultOptions(): ListingOptions {
  return { scope: 'path', kind: 'all', exact: false, caseSensitive: false, storageClass: '', includeDeleted: false, limit: 500 }
}

export function browserLocationKey(location: Location): string {
  return JSON.stringify([location.connection, location.bucket, location.prefix])
}

export function folderPrefix(value: string): string {
  return !value || value.endsWith('/') ? value : value + '/'
}

export function matchesListing(row: ObjectRow, prefix: string, phrase: string, options: ListingOptions): boolean {
  if (options.kind === 'object' && row.folder || options.kind === 'folder' && !row.folder) return false
  if (options.storageClass && (row.folder || row.deleteMarker || (row.storageClass ?? 'STANDARD') !== options.storageClass)) return false
  if (!phrase) return true
  const path = displayName(row.key, prefix, row.folder)
  const name = path.split('/').at(-1) || path
  const comparable = options.caseSensitive ? name : name.toLowerCase()
  const query = options.caseSensitive ? phrase : phrase.toLowerCase()
  if (options.exact) return comparable === query
  return comparable.includes(query) || (options.caseSensitive ? path : path.toLowerCase()).includes(query)
}

export function orderedRows(rows: ObjectRow[], sort: BrowserSort): ObjectRow[] {
  if (sort.column === 'provider') return rows
  return [...rows].sort((left, right) => {
    if (left.folder !== right.folder) return left.folder ? -1 : 1
    const order = compareRows(left, right, sort.column) || left.key.localeCompare(right.key)
    return sort.ascending ? order : -order
  })
}

function compareRows(left: ObjectRow, right: ObjectRow, column: SortColumn): number {
  if (column === 'size') return (left.size ?? -1) - (right.size ?? -1)
  if (column === 'modified') return (left.modified ?? '').localeCompare(right.modified ?? '')
  if (column === 'storageClass') return (left.storageClass ?? '').localeCompare(right.storageClass ?? '')
  return left.key.localeCompare(right.key, undefined, { numeric: true })
}

export function deletedListingRow(version: ObjectVersion, prefix: string, recursive: boolean): ObjectRow | undefined {
  if (!version.latest || !version.deleteMarker) return
  const childSeparator = version.key.indexOf('/', prefix.length)
  if (!recursive && childSeparator >= 0) return
  return { key: version.key, folder: false, size: null, modified: version.modified, storageClass: null, etag: null, deleteMarker: version.versionId }
}

export function selectedRange(keys: string[], anchor: string | null, key: string): string[] {
  const start = anchor === null ? -1 : keys.indexOf(anchor)
  const end = keys.indexOf(key)
  if (start < 0 || end < 0) return [key]
  return keys.slice(Math.min(start, end), Math.max(start, end) + 1)
}
