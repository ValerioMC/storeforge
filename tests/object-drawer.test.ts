// @vitest-environment jsdom

import { createApp, nextTick, reactive } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { objectsApi } from '../src/features/browser/api/objects'
import { verifyVersionChange } from '../src/features/browser/api/versions'
import BrowserWorkspace from '../src/features/browser/components/BrowserWorkspace.vue'
import ObjectInspector from '../src/features/browser/components/ObjectInspector.vue'
import { demoDetails } from '../src/features/browser/model/demo'
import { objectId, type ObjectRow, type ObjectVersion } from '../src/features/browser/model/objects'
import { parseCsv, previewText, searchPreview } from '../src/features/browser/model/preview'
import { useBrowser } from '../src/features/browser/store/browser'

vi.mock('../src/features/browser/api/objects', () => ({
  objectsApi: { list: vi.fn(), details: vi.fn(), tags: vi.fn(), preview: vi.fn(), versions: vi.fn(), acl: vi.fn(), change: vi.fn() },
}))

const target = objectId('archive', '/logs// / report.csv ')
let dispose: (() => void) | undefined

beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(objectsApi.details).mockResolvedValue(demoDetails(target.key))
  vi.mocked(objectsApi.preview).mockResolvedValue({ contentType: 'text/csv', kind: 'text', data: btoa('col\nvalue'), truncated: false, oversized: false })
  vi.mocked(objectsApi.versions).mockResolvedValue({ versions: [], cursor: null })
  HTMLDialogElement.prototype.showModal = function () { this.open = true }
  HTMLDialogElement.prototype.close = function () { this.open = false }
})

afterEach(() => { dispose?.(); dispose = undefined })

function mountInspector(object = target, deleted = false) {
  const props = reactive({ connection: 'test', object, deleted, demo: false, hasPrevious: false, hasNext: true })
  const host = document.createElement('div')
  document.body.append(host)
  const app = createApp(ObjectInspector, props)
  const inspector = app.mount(host) as unknown as { requestExit: (action: () => void) => void }
  dispose = () => { app.unmount(); host.remove() }
  return { host, inspector }
}

function clickButton(host: HTMLElement, label: string): void {
  const button = [...host.querySelectorAll('button'), ...document.querySelectorAll<HTMLButtonElement>('dialog[open] button')].find(item => item.textContent?.trim() === label || item.getAttribute('aria-label') === label)
  if (!button) throw new Error(`Button ${label} not found`)
  button.click()
}

async function settle(): Promise<void> {
  await nextTick()
  await new Promise(resolve => setTimeout(resolve, 120))
  await nextTick()
}

it('parses quoted CSV and treats truncated JSON as searchable plain text', () => {
  expect(parseCsv('key,value\r\n" leading ","a,b\n""quoted"""\r\n')).toEqual([['key', 'value'], [' leading ', 'a,b\n"quoted"']])
  expect(previewText('application/json', '{"literal": "', true)).toEqual({ raw: '{"literal": "', structured: null })
  expect(searchPreview('alpha ALPHA beta', 'alpha').filter(part => part.match)).toHaveLength(2)
})

it('opens a historical version with the exact identity and reports retrieval failure', async () => {
  const item: ObjectVersion = { key: target.key, versionId: 'history /+', latest: false, deleteMarker: false, size: 8, modified: null }
  vi.mocked(objectsApi.versions).mockResolvedValue({ versions: [item], cursor: null })
  vi.mocked(objectsApi.preview).mockResolvedValueOnce({ contentType: 'text/csv', kind: 'text', data: '', truncated: false, oversized: false }).mockRejectedValueOnce(new Error('Archive retrieval denied'))
  const { host } = mountInspector()
  await settle()
  clickButton(host, 'Versions')
  await settle()
  clickButton(host, 'View version')
  await settle()
  expect(objectsApi.preview).toHaveBeenLastCalledWith('test', { ...target, versionId: 'history /+' })
  expect(host.textContent).toContain('Archive retrieval denied')
  expect(host.textContent).toContain('history /+')
})

it('removes the precise current delete marker and reads the restored version from the provider', async () => {
  const marker: ObjectVersion = { key: target.key, versionId: 'marker /+', latest: true, deleteMarker: true, size: null, modified: null }
  const prior = { ...marker, versionId: 'older', deleteMarker: false, size: 8 }
  vi.mocked(objectsApi.versions).mockResolvedValueOnce({ versions: [marker, { ...prior, latest: false }], cursor: null })
  vi.mocked(objectsApi.versions).mockResolvedValue({ versions: [prior], cursor: null })
  const { host } = mountInspector({ ...target, versionId: marker.versionId }, true)
  await settle()
  expect(host.querySelector('button[aria-pressed="true"]')?.textContent).toBe('Versions')
  clickButton(host, 'Restore deleted object')
  await settle()
  clickButton(host, 'Remove marker')
  await settle()
  expect(objectsApi.change).toHaveBeenCalledWith('test', { ...target, versionId: 'marker /+' }, { kind: 'delete' })
  expect(host.textContent).toContain('older')
  expect(host.textContent).not.toContain('marker /+')
})

it('rejects a delete confirmation when the exact version remains on a later provider page', async () => {
  const version = { key: target.key, versionId: 'older /+ ', latest: false, deleteMarker: false, size: 8, modified: null }
  const cursor = { key: target.key, version: 'cursor /+ ' }
  vi.mocked(objectsApi.versions).mockResolvedValueOnce({ versions: [], cursor }).mockResolvedValueOnce({ versions: [version], cursor: null })

  await expect(verifyVersionChange('test', { ...target, versionId: version.versionId }, 'delete', undefined))
    .rejects.toThrow('The provider still lists the selected version')
  expect(objectsApi.versions).toHaveBeenLastCalledWith('test', target.bucket, target.key, true, cursor)
})

it('only confirms a historical restore when the provider lists the returned version as current', async () => {
  const previous = { key: target.key, versionId: 'old current', latest: true, deleteMarker: false, size: 8, modified: null }
  const restored = { ...previous, versionId: 'restored /+ ' }
  const concurrent = { ...previous, versionId: 'concurrent upload' }
  vi.mocked(objectsApi.versions)
    .mockResolvedValueOnce({ versions: [concurrent], cursor: null })
    .mockResolvedValueOnce({ versions: [restored], cursor: null })
  const source = { ...target, versionId: 'historical /+' }

  await expect(verifyVersionChange('test', source, 'restoreVersion', previous, restored.versionId))
    .rejects.toThrow('The provider has not confirmed the restored object as current')
  await expect(verifyVersionChange('test', source, 'restoreVersion', previous, restored.versionId))
    .resolves.toMatchObject({ latest: restored })
})

it('saves and verifies literal tag spaces on the viewed version', async () => {
  const original = [{ key: ' spaced key ', value: ' original ' }]
  const update = [{ key: ' spaced key ', value: ' updated ' }]
  const historical = { ...target, versionId: 'older /+ ' }
  vi.mocked(objectsApi.tags).mockResolvedValueOnce(original).mockResolvedValueOnce(update)
  const { host } = mountInspector(historical)
  await settle()
  clickButton(host, 'Tags')
  await settle()
  const field = host.querySelector<HTMLInputElement>('input[aria-label="Tag 1 value"]')!
  field.value = update[0]!.value
  field.dispatchEvent(new Event('input', { bubbles: true }))
  await nextTick()
  clickButton(host, 'Save tags')
  await settle()
  expect(objectsApi.change).toHaveBeenCalledWith('test', historical, { kind: 'setTags', tags: update })
  expect(objectsApi.tags).toHaveBeenNthCalledWith(2, 'test', historical)
  expect(field.value).toBe(update[0]!.value)
  expect(host.querySelector<HTMLButtonElement>('button[type="submit"], .panel-form button.primary')?.disabled).toBe(true)
})

it('retains a failed metadata edit until the operator explicitly discards it', async () => {
  vi.mocked(objectsApi.change).mockRejectedValue(new Error('Metadata update denied'))
  const { host, inspector } = mountInspector()
  await settle()
  clickButton(host, 'Details')
  await settle()
  clickButton(host, 'Edit metadata')
  await settle()
  const field = host.querySelector('textarea')!
  field.value = '{"literal":" keep "}'
  field.dispatchEvent(new Event('input', { bubbles: true }))
  await nextTick()
  clickButton(host, 'Save metadata')
  await settle()
  expect(host.textContent).toContain('Metadata update denied')
  expect(field.value).toBe('{"literal":" keep "}')
  const leave = vi.fn()
  inspector.requestExit(leave)
  await settle()
  clickButton(host, 'Keep editing')
  expect(leave).not.toHaveBeenCalled()
  expect(field.value).toBe('{"literal":" keep "}')
  inspector.requestExit(leave)
  await settle()
  clickButton(host, 'Discard draft')
  expect(leave).toHaveBeenCalledOnce()
})

it('opens a row menu and moves between objects without changing checked targets', async () => {
  const keys = [target.key, '/logs// / second.json ']
  const rows: ObjectRow[] = keys.map(key => ({ key, folder: false, size: 12, modified: null, storageClass: null, etag: null }))
  vi.mocked(objectsApi.list).mockResolvedValue({ rows, cursor: null })
  const pinia = createPinia()
  setActivePinia(pinia)
  const browser = useBrowser()
  await browser.changeConnection('demo', 'scope-2')
  await browser.navigate(target.bucket)
  const host = document.createElement('div')
  document.body.append(host)
  const app = createApp(BrowserWorkspace, { demo: true, compact: false }).use(pinia)
  app.mount(host)
  dispose = () => { app.unmount(); host.remove() }
  await settle()

  host.querySelector<HTMLInputElement>('input[aria-label^="Select /logs"]')!.click()
  clickButton(host, 'Actions for ' + target.key)
  await settle()
  clickButton(document.body, 'Object details')
  await settle()
  expect(objectsApi.details).toHaveBeenCalledWith('demo', target)
  clickButton(host, 'Next object')
  await settle()
  expect(objectsApi.preview).toHaveBeenLastCalledWith('demo', objectId(target.bucket, keys[1]!))
  expect(browser.selectedObjects).toEqual([target])
  clickButton(host, 'Close inspector')
  await settle()
  expect(document.activeElement?.getAttribute('title')).toBe(keys[1])
  expect(browser.selectedObjects).toEqual([target])
})
