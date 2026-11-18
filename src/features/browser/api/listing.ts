import { deletedListingRow, matchesListing, orderedRows, type BrowserSort, type ListingOptions } from '../model/listing'
import type { Location, ObjectRow, VersionCursor } from '../model/objects'
import { objectsApi } from './objects'

interface ListingRequest {
  target: Location
  phrase: string
  recursive: boolean
  options: ListingOptions
  sort: BrowserSort
}

export interface ListingResult {
  rows: ObjectRow[]
  limitReached: boolean
  deletedError: string
}

export async function* readObjectPages(target: Location, recursive: boolean, current: () => boolean) {
  let cursor: string | null = null
  const received = new Set<string>()
  do {
    const page = await objectsApi.list(target, cursor, recursive)
    if (!current()) return
    yield page.rows
    cursor = page.cursor
    if (cursor === null) return
    if (received.has(cursor)) throw new Error('The endpoint repeated an object listing continuation token.')
    received.add(cursor)
  } while (current())
}

async function* readDeletedPages(target: Location, recursive: boolean, current: () => boolean) {
  let cursor: VersionCursor | null = null
  const received = new Set<string>()
  do {
    const page = await objectsApi.versions(target.connection, target.bucket, target.prefix, false, cursor)
    if (!current()) return
    yield page.versions.flatMap(version => {
      const row = deletedListingRow(version, target.prefix, recursive)
      return row ? [row] : []
    })
    cursor = page.cursor
    if (cursor === null) return
    const marker = JSON.stringify(cursor)
    if (received.has(marker)) throw new Error('The endpoint repeated a version listing continuation token.')
    received.add(marker)
  } while (current())
}

export async function searchListing(request: ListingRequest, current: () => boolean): Promise<ListingResult> {
  const { phrase, options, sort } = request
  const target = { ...request.target, prefix: options.scope === 'bucket' ? '' : request.target.prefix }
  let rows: ObjectRow[] = []
  let limitReached = false
  let deletedError = ''
  function include(page: ObjectRow[]): boolean {
    const matching = page.filter(row => matchesListing(row, target.prefix, phrase, options))
    const ordered = orderedRows([...rows, ...matching], sort)
    if (ordered.length > options.limit) limitReached = true
    rows = ordered.slice(0, options.limit)
    return limitReached && sort.column === 'provider'
  }

  for await (const page of readObjectPages(target, request.recursive, current)) {
    if (include(page) && !options.includeDeleted) break
  }
  if (current() && options.includeDeleted) {
    try {
      for await (const page of readDeletedPages(target, request.recursive, current)) {
        if (include(page)) break
      }
    } catch (error) {
      deletedError = typeof error === 'object' && error !== null && 'message' in error ? String(error.message) : String(error)
    }
  }
  return { rows, limitReached, deletedError }
}

export async function countPrefix(target: Location, current: () => boolean): Promise<{ objects: number; bytes: number }> {
  let objects = 0
  let bytes = 0
  for await (const page of readObjectPages(target, true, current)) {
    for (const row of page) {
      if (!row.folder) {
        objects++
        bytes += row.size ?? 0
      }
    }
  }
  return { objects, bytes }
}
