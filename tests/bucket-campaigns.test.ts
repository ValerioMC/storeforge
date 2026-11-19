// @vitest-environment jsdom

import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createApp, h, nextTick } from 'vue'
import { campaignGateway } from '../src/features/bucket/api/campaigns'
import BucketCampaign from '../src/features/bucket/components/BucketCampaign.vue'
import { useBucketCampaign } from '../src/features/bucket/composables/useBucketCampaign'
import { applyCampaignItem, captureCampaign, previewCampaignTarget, readCampaignBackup, snapshotPatch, writeCampaignBackup,
  type CampaignGateway, type CampaignPatch } from '../src/features/bucket/model/campaign'
import { makeWorkbenchRow } from '../src/features/bucket/model/workbench'
import { endpointSettings, type Connection } from '../src/features/connection/model/connection'

vi.mock('../src/features/bucket/api/campaigns', () => ({ campaignGateway: {
  read: vi.fn(), quota: vi.fn(), configure: vi.fn(), updateQuota: vi.fn(), usage: vi.fn(),
} }))

const currentLifecycle = `<LifecycleConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule><ID>keep</ID><Status>Enabled</Status><Expiration><Days>90</Days></Expiration></Rule>
  <Rule><ID>update</ID><Status>Disabled</Status><Expiration><Days>4</Days></Expiration></Rule>
  <Rule><ID>remove</ID><Status>Enabled</Status><Expiration><Days>2</Days></Expiration></Rule>
</LifecycleConfiguration>`
const desiredLifecycle = `<LifecycleConfiguration>
  <Rule><ID>update</ID><Status>Enabled</Status><Expiration><Days>4</Days></Expiration></Rule>
  <Rule><ID>new</ID><Status>Enabled</Status><Expiration><Days>14</Days></Expiration></Rule>
</LifecycleConfiguration>`
const mergePatch: CampaignPatch = { sections: [{ section: 'lifecycle', operation: 'merge', document: desiredLifecycle, removeIds: ['remove'] }] }
const beforeQuota = { enabled: true, maxBytes: '9007199254740993', maxObjects: '12' }
let mounted: ReturnType<typeof createApp> | undefined

function row(id: string, available = true, privileged = false) {
  const name = id.endsWith('source') ? id : 'same-bucket'
  const profile: Connection = {
    id, name: id, provider: 'ceph', endpoint: 'https://storage.example.test', region: 'us-east-1',
    pathStyle: true, knownBuckets: [], revision: `${id}-revision`, settings: { ...endpointSettings(), tenant: id },
    s3Capability: available ? 'detected' : 'unavailable',
    services: privileged ? { cephAdmin: { principalId: `${id}-quota`, endpoint: 'https://storage.example.test',
      enabled: true, managedExternally: false, capability: 'detected' } } : undefined,
  }
  return makeWorkbenchRow(profile, 'storageOps', { name, tenant: id, instance: null, owner: null,
    bytes: null, objects: null, createdAt: null, quota: null })
}

function mockGateway() {
  return {
    read: vi.fn<CampaignGateway['read']>(),
    quota: vi.fn<CampaignGateway['quota']>().mockResolvedValue(beforeQuota),
    configure: vi.fn<CampaignGateway['configure']>(),
    updateQuota: vi.fn<CampaignGateway['updateQuota']>().mockImplementation(async (_, __, draft) => draft),
    usage: vi.fn<CampaignGateway['usage']>(),
  }
}

function button(label: string): HTMLButtonElement {
  const element = [...document.querySelectorAll('button')].find(item => item.textContent?.trim() === label || item.ariaLabel === label)
  if (!element) throw new Error(`Missing button: ${label}`)
  return element
}

async function choose(label: string, value: string): Promise<void> {
  button(label).click()
  await nextTick()
  const option = [...document.querySelectorAll<HTMLElement>('[role=option]')].find(item => item.textContent?.trim() === value)
  expect(option).toBeDefined()
  option!.click()
  await nextTick()
}

beforeEach(() => {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true, value(this: HTMLDialogElement) { this.open = true },
  })
})

afterEach(() => {
  mounted?.unmount()
  mounted = undefined
  document.body.replaceChildren()
  vi.resetAllMocks()
})

it('distinguishes unsupported, denied and unchanged targets and applies only confirmed rule changes', async () => {
  const stored = new Map<string, string>()
  const gateway = mockGateway()
  const changed = row('changed')
  const retry = row('retry')
  const untouched = row('untouched')
  const unavailable = row('unavailable', false)
  const denied = row('denied')
  gateway.read.mockImplementation(async target => {
    if (target.key === denied.key) throw new Error('403 Forbidden')
    return stored.get(target.key) ?? currentLifecycle
  })
  gateway.configure.mockImplementation(async (target, update) => {
    if (target.key === retry.key && !stored.has(target.key)) {
      stored.set(target.key, currentLifecycle)
      throw new Error('Gateway temporarily unavailable')
    }
    const result = update.change.action === 'write' ? update.change.document : ''
    stored.set(target.key, result)
    return result
  })
  const campaign = useBucketCampaign(gateway, () => {})
  await campaign.preview([changed, retry, untouched, unavailable, denied].map(target => ({ row: target, patch: mergePatch })))
  expect(campaign.targets.value.map(target => target.items[0]?.status)).toEqual(['ready', 'ready', 'ready', 'unsupported', 'denied'])
  expect(gateway.read).not.toHaveBeenCalledWith(unavailable, 'lifecycle')
  campaign.setConfirmed(changed.key, true)
  campaign.setConfirmed(retry.key, true)
  await campaign.apply()
  expect(gateway.configure.mock.calls.map(([target]) => target.key)).toEqual([changed.key, retry.key])
  const document = stored.get(changed.key)
  expect(document).toContain('<ID>keep</ID>')
  expect(document).toContain('<ID>new</ID>')
  expect(document).toMatch(/<ID>update<\/ID><Status>Enabled<\/Status>/)
  expect(document).not.toContain('<ID>remove</ID>')
  expect(campaign.targets.value.map(target => target.items[0]?.status)).toEqual(['applied', 'error', 'skipped', 'unsupported', 'denied'])

  await campaign.retry()
  expect(campaign.targets.value.map(target => target.items[0]?.status)).toEqual(['applied', 'ready', 'skipped', 'unsupported', 'denied'])
  expect(campaign.selected.value).toHaveLength(0)
  campaign.setConfirmed(retry.key, true)
  await campaign.apply()
  expect(gateway.configure.mock.calls.map(([target]) => target.key)).toEqual([changed.key, retry.key, retry.key])
  await campaign.preview([changed, retry].map(target => ({ row: target, patch: mergePatch })))
  expect(campaign.targets.value.map(target => target.items[0]?.status)).toEqual(['unchanged', 'unchanged'])
  expect(campaign.eligible.value).toHaveLength(0)
})

it('merges notification IDs and clears the last notification with a supported write', async () => {
  const gateway = mockGateway()
  const target = row('notification')
  gateway.read.mockResolvedValue('<NotificationConfiguration><QueueConfiguration><Id>clear</Id><Queue>arn:queue:old</Queue></QueueConfiguration><TopicConfiguration><Id>keep</Id><Topic>arn:topic:keep</Topic></TopicConfiguration></NotificationConfiguration>')
  const merged = await previewCampaignTarget(target, { sections: [{ section: 'notification', operation: 'merge', removeIds: ['clear'],
    document: '<NotificationConfiguration><CloudFunctionConfiguration><Id>new</Id><CloudFunction>arn:function:new</CloudFunction></CloudFunctionConfiguration></NotificationConfiguration>' }] }, gateway)
  expect(merged.items[0]?.after).toContain('<Id>keep</Id>')
  expect(merged.items[0]?.after).toContain('<Id>new</Id>')
  expect(merged.items[0]?.after).not.toContain('<Id>clear</Id>')
  const removed = await previewCampaignTarget(target, { sections: [{ section: 'notification', operation: 'remove', document: '' }] }, gateway)
  expect(removed.items[0]?.action).toMatchObject({ update: { change: { action: 'write', document: expect.stringContaining('NotificationConfiguration') } } })
})

it('round-trips a scoped clipboard backup, selectively pastes settings and preserves exact quota limits', async () => {
  const gateway = mockGateway()
  const source = row('manual-source', true, true)
  const target = row('target', true, true)
  gateway.read.mockImplementation(async (_, section) => section === 'policy'
    ? '{"Statement":{"Effect":"Deny","Resource":["arn:aws:s3:::manual-source","arn:aws:s3:::manual-source/*","arn:aws:s3:::independent/*"]}}'
    : '')
  const { snapshot, items } = await captureCampaign(source, ['policy', 'quota', 'lifecycle'], gateway)
  expect(items.map(item => item.status)).toEqual(['applied', 'applied', 'unchanged'])
  const parsed = readCampaignBackup(writeCampaignBackup(snapshot))
  expect(parsed.quota?.maxBytes).toBe('9007199254740993')
  expect(snapshotPatch(parsed, ['policy'], target)).toEqual({ sections: [{
    section: 'policy', operation: 'replace', document: expect.stringContaining('arn:aws:s3:::same-bucket/*'),
  }], quota: undefined })
  expect(snapshotPatch(parsed, ['policy'], target).sections[0]?.document).toContain('arn:aws:s3:::independent/*')
  const [blocked, permitted] = await Promise.all([
    previewCampaignTarget(row('standard'), { sections: [], quota: { maxObjects: '42' } }, gateway),
    previewCampaignTarget(target, { sections: [], quota: { maxObjects: '42' } }, gateway),
  ])
  expect(blocked.items[0]?.status).toBe('unsupported')
  expect(gateway.quota).not.toHaveBeenCalledWith(blocked.row)
  const applied = await applyCampaignItem(target, permitted.items[0]!, gateway)
  expect(applied.quota).toEqual({ ...beforeQuota, maxObjects: '42' })
})

it('keeps partial scan results visible with their coverage', async () => {
  const gateway = mockGateway()
  const target = row('usage')
  const review = await previewCampaignTarget(target, { sections: [], usage: true }, gateway)
  expect(gateway.usage).not.toHaveBeenCalled()
  gateway.usage.mockResolvedValue({ coverage: 'partial', detail: 'List permission expired', value: {
    currentBytes: 24, currentObjects: 2, noncurrentBytes: null, noncurrentObjects: null, deleteMarkers: null,
    sizes: {}, ages: {}, types: {}, storageClasses: {},
  } })
  const result = await applyCampaignItem(target, review.items[0]!, gateway)
  expect(result.status).toBe('partial')
  expect(result.summary).toContain('2 objects')
  expect(result.summary).toContain('List permission expired')
})

it('previews a separately mapped source for each target and only enables apply after confirmation', async () => {
  vi.mocked(campaignGateway.read).mockImplementation(async (target, section) => {
    if (!target.name.endsWith('source') || section !== 'versioning') return ''
    return `<VersioningConfiguration><Status>${target.name === 'one-source' ? 'Enabled' : 'Suspended'}</Status></VersioningConfiguration>`
  })
  const sources = [row('one-source'), row('two-source')]
  const selection = [row('north'), row('south')]
  const rows = [...sources, ...selection]
  const host = document.createElement('div')
  document.body.append(host)
  mounted = createApp({ setup: () => () => h(BucketCampaign, { rows, selection }) })
  mounted.mount(host)
  await choose('Campaign action', 'Copy from bucket')
  await choose('Source mapping', 'One source per target')
  await choose('Source for target 1', 'one-source · one-source · one-source')
  await choose('Source for target 2', 'two-source · two-source · two-source')
  button('Preview changes').click()
  await vi.waitFor(() => expect(document.querySelectorAll('.campaign-target-header')).toHaveLength(2))
  const reviews = [...document.querySelectorAll('.campaign-target-header')].map(header => header.parentElement?.textContent)
  expect(reviews[0]).toContain('Set versioning to Enabled')
  expect(reviews[1]).toContain('Set versioning to Suspended')
  expect(button('Apply to 0 targets').disabled).toBe(true)
  expect(campaignGateway.configure).not.toHaveBeenCalled()
  document.querySelector<HTMLInputElement>('[aria-label^="Confirm same-bucket · south"]')!.click()
  await nextTick()
  expect(button('Apply to 1 target').disabled).toBe(false)
})
