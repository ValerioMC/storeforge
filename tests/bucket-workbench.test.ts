// @vitest-environment jsdom

import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createApp, h, nextTick, ref } from 'vue'
import { saveBucketSelection, scopedBucketConfiguration, streamBucketInventory } from '../src/features/bucket/api/workbench'
import BucketWorkbench from '../src/features/bucket/components/BucketWorkbench.vue'
import { useBucketClassifications } from '../src/features/bucket/composables/useBucketClassifications'
import { makeWorkbenchRow, type InventoryBucket } from '../src/features/bucket/model/workbench'
import { endpointSettings, type Connection } from '../src/features/connection/model/connection'
import { isWorkspaceRoute } from '../src/features/connection/model/workspace'
import { readMonitoring } from '../src/features/monitoring/api/monitoring'

vi.mock('../src/features/bucket/api/workbench')
vi.mock('../src/features/monitoring/api/monitoring')

const north = profile('north', 'North', 'tenant-north')
const south = profile('south', 'South', 'tenant-south')
const bucket = (name: string, tenant = ''): InventoryBucket => ({
  name, tenant, instance: null, owner: null, quota: null, bytes: null, objects: null, createdAt: null,
})
const lifecycle = '<LifecycleConfiguration><Rule><ID>retention</ID><Status>Disabled</Status><Expiration><Days>7</Days></Expiration></Rule></LifecycleConfiguration>'
const encrypted = '<ServerSideEncryptionConfiguration><Rule><ApplyServerSideEncryptionByDefault><SSEAlgorithm>aws:kms</SSEAlgorithm><KMSMasterKeyID>east:key/32</KMSMasterKeyID></ApplyServerSideEncryptionByDefault></Rule></ServerSideEncryptionConfiguration>'
let mounted: ReturnType<typeof createApp> | undefined

function profile(id: string, name: string, tenant: string): Connection {
  return { id, name, provider: 'ceph', endpoint: 'https://storage.example.test', region: 'zone-a',
    pathStyle: true, knownBuckets: [], revision: 'rev-1', endpointId: 'physical', settings: { ...endpointSettings(), tenant } }
}

function button(label: string, parent: ParentNode = document): HTMLButtonElement {
  const result = [...parent.querySelectorAll('button')].find(item => item.textContent?.trim() === label || item.getAttribute('aria-label') === label)
  if (!result) throw new Error(`Missing button: ${label}`)
  return result
}

function edit(input: HTMLInputElement, value: string): void {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

async function choose(label: string, value: string): Promise<void> {
  await nextTick()
  button(label).click()
  await vi.waitFor(() => {
    const option = [...document.querySelectorAll<HTMLElement>('[role=option]')].find(item => item.textContent?.trim() === value)
    expect(option).toBeDefined()
    option!.click()
  })
}

function show(profiles: Connection[] = [north, south]) {
  const target = ref<string>()
  const host = document.createElement('div')
  document.body.append(host)
  mounted = createApp({
    setup: () => () => h(BucketWorkbench, {
      profiles, source: 'storageOps', pane: 'buckets', target: target.value, link: 'http://localhost/#bucketWorkbench',
      onInspect: (key?: string) => { target.value = key },
    }),
  })
  mounted.mount(host)
  return target
}

beforeEach(() => {
  const values = new Map<string, string>()
  Object.defineProperty(window, 'localStorage', { configurable: true, value: {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value) },
  } })
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true, value(this: HTMLDialogElement) { this.open = true },
  })
  vi.mocked(streamBucketInventory).mockImplementation(async (profile, _, receive) => {
    receive([bucket('same-name', profile.settings!.tenant)])
  })
  vi.mocked(readMonitoring).mockImplementation(async id => ({
    scope: { connectionId: id, revision: 'rev-1', endpointId: 'physical', tenant: '', account: '', region: 'zone-a' },
    retrievedAt: '', stale: false, usage: null, hourly: [], daily: [], health: [], incidents: [],
  }))
  vi.mocked(scopedBucketConfiguration).mockImplementation(async (target, section) => {
    if (target.connection === 'north' && section === 'lifecycle') return lifecycle
    if (target.connection === 'south' && section === 'encryption') return encrypted
    return ''
  })
  vi.mocked(saveBucketSelection).mockResolvedValue(true)
})

afterEach(() => {
  mounted?.unmount()
  mounted = undefined
  document.body.replaceChildren()
  vi.clearAllMocks()
})

it('filters equal bucket names by lifecycle state and KMS key and exports visible columns', async () => {
  show()
  await vi.waitFor(() => expect(document.querySelectorAll('.register-panel tbody tr')).toHaveLength(2))
  const choice = [...document.querySelectorAll<HTMLInputElement>('.columns-menu input')]
  choice.find(input => input.parentElement?.textContent?.trim() === 'Lifecycle rules')!.click()
  choice.find(input => input.parentElement?.textContent?.trim() === 'KMS key')!.click()
  button('Filters').click()
  await vi.waitFor(() => button('Add filter')).then(control => control.click())
  await nextTick()
  edit(document.querySelector<HTMLInputElement>('[aria-label="Filter 1 value"]')!, 'Disabled')
  document.querySelector('dialog form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
  await vi.waitFor(() => expect(document.querySelector('.register-panel tbody')?.textContent).toContain('North'))
  expect(document.querySelector('.register-panel tbody')?.textContent).not.toContain('South')
  button('Select filtered').click()
  await vi.waitFor(() => expect(button('Export selection').disabled).toBe(false))
  button('Export selection').click()
  await vi.waitFor(() => expect(saveBucketSelection).toHaveBeenCalledWith('csv', expect.stringContaining('"Lifecycle rules","KMS key"')))
  expect(vi.mocked(saveBucketSelection).mock.calls[0][1]).toContain('"same-name","North","tenant-north"')

  await choose('Export format', 'JSON')
  button('Clear selection').click()
  document.querySelector<HTMLButtonElement>('.workbench-controls button:not([role=combobox])')!.click()
  await choose('Filter 1 field', 'KMS key')
  edit(document.querySelector<HTMLInputElement>('[aria-label="Filter 1 value"]')!, 'east:key/32')
  document.querySelector('dialog form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
  await vi.waitFor(() => expect(document.querySelector('.register-panel tbody')?.textContent).toContain('South'))
  expect(document.querySelector('.register-panel tbody')?.textContent).not.toContain('North')
  button('Select filtered').click()
  await vi.waitFor(() => expect(button('Export selection').disabled).toBe(false))
  button('Export selection').click()
  await vi.waitFor(() => expect(saveBucketSelection).toHaveBeenCalledTimes(2))
  expect(JSON.parse(vi.mocked(saveBucketSelection).mock.calls[1][1])).toEqual([
    expect.objectContaining({ Bucket: 'same-name', Context: 'South', 'KMS key': 'east:key/32' }),
  ])
})

it('keeps the page, search, scroll and selection while navigating a scoped drawer', async () => {
  vi.mocked(streamBucketInventory).mockImplementation(async (profile, _, receive) => {
    receive(Array.from({ length: 32 }, (_, index) => bucket(`same-${String(index).padStart(2, '0')}`, profile.settings!.tenant)))
  })
  const target = show()
  const search = document.querySelector<HTMLInputElement>('[aria-label="Search bucket names"]')!
  edit(search, 'same-')
  await vi.waitFor(() => expect(button('Next page').disabled).toBe(false))
  button('Next page').click()
  await vi.waitFor(() => expect(document.querySelector('.workbench-pagination')?.textContent).toContain('Page 2 of 3'))
  const table = document.querySelector<HTMLElement>('.register-panel .workbench-table')!
  table.scrollLeft = 73
  const chooseBucket = table.querySelector<HTMLInputElement>('tbody input')!
  chooseBucket.click()
  button('same-15', table).click()
  await vi.waitFor(() => expect(document.querySelector('dialog')?.textContent).toContain('North · tenant-north'))
  expect(isWorkspaceRoute({ workspace: 'storageOps', page: 'buckets', connectionId: 'north', workbenchBucket: target.value })).toBe(true)
  expect(vi.mocked(scopedBucketConfiguration).mock.calls.at(-1)?.[0]).toMatchObject({ connection: 'north', tenant: 'tenant-north', name: 'same-15' })
  button('Next bucket').click()
  await vi.waitFor(() => expect(document.querySelector('dialog')?.textContent).toContain('South · tenant-south'))
  expect(vi.mocked(scopedBucketConfiguration).mock.calls.at(-1)?.[0]).toMatchObject({ connection: 'south', tenant: 'tenant-south', name: 'same-15' })
  button('Close dialog').click()
  await vi.waitFor(() => expect(document.querySelector('dialog')).toBeNull())
  expect(document.querySelector('.workbench-pagination')?.textContent).toContain('Page 2 of 3')
  expect(search.value).toBe('same-')
  expect(chooseBucket.checked).toBe(true)
  expect(table.scrollLeft).toBe(73)
})

it('shares local tags by physical endpoint within the workspace and tenant', () => {
  const classifications = useBucketClassifications('storageOps')
  const original = makeWorkbenchRow(north, 'storageOps', bucket('same-name', 'tenant-north'))
  const alias = makeWorkbenchRow(profile('other', 'Other', 'tenant-north'), 'storageOps', bucket('same-name', 'tenant-north'))
  const otherTenant = makeWorkbenchRow(south, 'storageOps', bucket('same-name', 'tenant-south'))
  classifications.saveTags(original, 'archive, production, archive')
  expect(classifications.tags(alias)).toEqual(['archive', 'production'])
  expect(classifications.tags(otherTenant)).toEqual([])
  expect(useBucketClassifications('cephAdmin').tags(original)).toEqual([])
  expect(scopedBucketConfiguration).not.toHaveBeenCalled()
})
