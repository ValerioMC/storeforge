// @vitest-environment jsdom

import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, expect, it, vi } from 'vitest'
import { objectsApi } from '../src/features/browser/api/objects'
import { demoDetails } from '../src/features/browser/model/demo'
import { browserLocationKey, orderedRows } from '../src/features/browser/model/listing'
import type { ObjectRow } from '../src/features/browser/model/objects'
import { useBrowser } from '../src/features/browser/store/browser'

vi.mock('../src/features/browser/api/objects', () => ({
  objectsApi: { list: vi.fn(), details: vi.fn(), versions: vi.fn() },
}))

function row(key: string, size = 1): ObjectRow {
  return { key, folder: false, size, modified: null, storageClass: 'STANDARD', etag: null }
}

beforeEach(() => {
  const data = new Map<string, string>()
  Object.defineProperty(window, 'localStorage', { configurable: true, value: {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => { data.set(key, value) },
  } })
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

it('searches across provider pages with an exact literal prefix and filename', async () => {
  const browser = useBrowser()
  vi.mocked(objectsApi.list).mockImplementation(async (target, cursor) => cursor
    ? { rows: [row(target.prefix + ' report.txt ')], cursor: null }
    : { rows: [row(target.prefix + ' REPORT.txt ')], cursor: 'second' })
  await browser.changeConnection('demo')
  await browser.navigate('archive', '/logs// /')
  browser.filter = ' report.txt '
  browser.options.exact = true
  browser.options.caseSensitive = true
  await browser.loadRows()

  expect(objectsApi.list).toHaveBeenLastCalledWith({ connection: 'demo', bucket: 'archive', prefix: '/logs// /' }, 'second', false)
  expect(browser.rows.map(item => item.key)).toEqual(['/logs// / report.txt '])
})

it('returns the largest object across pages when a non-provider sort is selected', async () => {
  const browser = useBrowser()
  vi.mocked(objectsApi.list).mockImplementation(async (_, cursor) => cursor
    ? { rows: [row('large', 8)], cursor: null }
    : { rows: [row('small', 1)], cursor: 'later' })
  await browser.changeConnection('demo')
  await browser.navigate('archive')
  browser.options.limit = 1
  browser.sort = { column: 'size', ascending: false }
  await browser.loadRows()
  expect(browser.rows.map(item => item.key)).toEqual(['large'])
  expect(browser.limitReached).toBe(true)
  expect(orderedRows([row('z'), row('a')], { column: 'provider', ascending: true }).map(item => item.key)).toEqual(['z', 'a'])
})

it('keeps row focus, checked range and the opened object independent of column changes', async () => {
  const browser = useBrowser()
  const rows = ['a', 'b', 'c'].map(key => row(key))
  vi.mocked(objectsApi.list).mockResolvedValue({ rows, cursor: null })
  vi.mocked(objectsApi.details).mockResolvedValue(demoDetails('b'))
  await browser.changeConnection('demo')
  await browser.navigate('archive')
  browser.focus = { bucket: 'archive', key: 'b', versionId: null }
  browser.focusedRow = 'c'
  browser.toggleSelection('a', ['a', 'b', 'c'], false)
  browser.toggleSelection('c', ['a', 'b', 'c'], true)
  await browser.loadRowDetails('b')
  browser.columnWidths.modified = 160
  await browser.loadRowDetails('b')
  expect(browser.selected).toEqual(['a', 'b', 'c'])
  expect(browser.focus?.key).toBe('b')
  expect(browser.focusedRow).toBe('c')
  expect(objectsApi.details).toHaveBeenCalledTimes(1)
})

it('keeps a favorite keyed to its saved connection after it is absent from the current context', async () => {
  const browser = useBrowser()
  vi.mocked(objectsApi.list).mockResolvedValue({ rows: [], cursor: null })
  await browser.changeConnection('demo')
  await browser.navigate('archive', '/ /')
  browser.toggleFavorite()
  await browser.changeConnection('')
  setActivePinia(createPinia())
  const reopened = useBrowser()
  expect(reopened.favorites.map(browserLocationKey)).toEqual([browserLocationKey({ connection: 'demo', bucket: 'archive', prefix: '/ /' })])
  expect(reopened.location.connection).toBe('')
})

it('shows only current delete markers from all version pages and keeps live results if access is denied', async () => {
  const browser = useBrowser()
  vi.mocked(objectsApi.list).mockResolvedValue({ rows: [row('present')], cursor: null })
  vi.mocked(objectsApi.versions).mockResolvedValueOnce({
    versions: [{ key: 'present', versionId: 'old-marker', latest: false, deleteMarker: true, size: null, modified: null }],
    cursor: { key: 'removed ', version: 'next' },
  }).mockResolvedValueOnce({
    versions: [{ key: 'removed ', versionId: 'current-marker', latest: true, deleteMarker: true, size: null, modified: null }],
    cursor: null,
  })
  await browser.changeConnection('demo')
  await browser.navigate('archive')
  browser.options.includeDeleted = true
  await browser.loadRows()
  expect(browser.rows.map(item => [item.key, item.deleteMarker])).toEqual([['present', undefined], ['removed ', 'current-marker']])
  vi.mocked(objectsApi.versions).mockRejectedValueOnce(new Error('Access denied'))
  await browser.loadRows()
  expect(browser.rows.map(item => item.key)).toEqual(['present'])
  expect(browser.deletedError).toBe('Access denied')
})

it('persists listing preferences for the exact connection revision and path', async () => {
  vi.mocked(objectsApi.list).mockResolvedValue({ rows: [], cursor: null })
  const browser = useBrowser()
  await browser.changeConnection('connection-a', 'revision-a')
  await browser.navigate('archive', '/ /')
  browser.recursive = true
  browser.columnWidths.modified = 170
  setActivePinia(createPinia())
  const reopened = useBrowser()
  await reopened.changeConnection('connection-a', 'revision-a')
  await reopened.navigate('archive', '/ /')
  expect(reopened.recursive).toBe(true)
  expect(reopened.columnWidths.modified).toBe(170)
  await reopened.changeConnection('connection-a', 'revision-b')
  await reopened.navigate('archive', '/ /')
  expect(reopened.recursive).toBe(false)
  expect(reopened.columnWidths.modified).toBeUndefined()
})
