// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, nextTick, type Component } from 'vue'
import { bucketsApi } from '../src/features/bucket/api/buckets'
import BucketBackupDialog from '../src/features/bucket/components/BucketBackupDialog.vue'
import BucketSettings from '../src/features/bucket/components/BucketSettings.vue'
import NewBucketDialog from '../src/features/bucket/components/NewBucketDialog.vue'
import { useBucketConfiguration } from '../src/features/bucket/composables/useBucketConfiguration'
import type { Configuration, ConfigurationChange } from '../src/features/bucket/model/bucket'
import { configurations } from '../src/features/bucket/model/configuration'
import { extractRules, readBackup, scanConfigurations, setLifecycleRuleStatus, type ConfigurationReading } from '../src/features/bucket/model/workspace'
import { readXml, setXmlValue, writeXml, xmlChildren } from '../src/features/bucket/model/xml-document'
import type { Connection } from '../src/features/connection/model/connection'

const tagging = '<Tagging xmlns:e="urn:extension"><TagSet><Tag><Key>  literal key </Key><Value> value  </Value><e:Detail code="retain"> Unknown </e:Detail></Tag></TagSet></Tagging>'
const lifecycle = '<LifecycleConfiguration xmlns:e="urn:extension"><Rule><ID>Retire reports</ID><Filter><Prefix> reports/</Prefix></Filter><Status>Disabled</Status><Expiration><Days>42</Days></Expiration><e:Extra>keep</e:Extra></Rule></LifecycleConfiguration>'
const versioning = '<VersioningConfiguration><Status>Enabled</Status></VersioningConfiguration>'
const profile: Connection = {
  id: 'fixture', name: 'Fixture', provider: 'ceph', endpoint: 'https://fixture.invalid',
  region: 'zone-a', pathStyle: true, knownBuckets: [],
}
const cleanups: Array<() => void> = []

beforeEach(() => {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true,
    value(this: HTMLDialogElement) { this.open = true },
  })
})

afterEach(() => {
  cleanups.splice(0).reverse().forEach(cleanup => cleanup())
  vi.restoreAllMocks()
})

function mount(component: Component, props: Record<string, unknown>): HTMLDivElement {
  const host = document.body.appendChild(document.createElement('div'))
  const app = createApp({ render: () => h(component, props) })
  app.mount(host)
  cleanups.push(() => {
    app.unmount()
    host.remove()
  })
  return host
}

function button(root: ParentNode, label: string): HTMLButtonElement {
  const item = Array.from(root.querySelectorAll('button')).find(button => button.textContent?.trim() === label)
  if (!item) throw new Error(`Missing button ${label}`)
  return item
}

describe('bucket configuration workspace', () => {
  it('round-trips provider-specific fields and literal tag spaces through a backup and form edit', () => {
    const backup = readBackup(JSON.stringify({
      format: 'storeforge-bucket-config', version: 1, bucket: 'original', documents: { tagging },
    }))
    expect(backup.documents.tagging).toBe(tagging)
    const root = readXml(backup.documents.tagging!, 'Tagging').documentElement
    const tag = xmlChildren(xmlChildren(root, 'TagSet')[0]!, 'Tag')[0]!
    setXmlValue(tag, { kind: 'text', name: 'Value', label: 'Value', allowEmpty: true }, ' updated value  ', xmlChildren(tag, 'Value')[0])
    const saved = writeXml(root)
    expect(saved).toContain('<e:Detail code="retain"> Unknown </e:Detail>')
    expect(extractRules('tagging', saved)).toMatchObject([{ name: '  literal key ', detail: ' updated value  ' }])
  })

  it('keeps independent drafts through a section save and refresh while exposing read failures', async () => {
    const documents = new Map<Configuration, string>([['tagging', tagging], ['lifecycle', lifecycle], ['versioning', versioning]])
    const request = vi.fn(async (section: Configuration, change: ConfigurationChange) => {
      if (change.action === 'write') documents.set(section, change.document)
      if (section === 'tagging' && documents.get(section) === 'denied') throw new Error('Access denied')
      return documents.get(section) ?? ''
    })
    const editor = useBucketConfiguration(request)
    await editor.refresh('tagging')
    await editor.refresh('lifecycle')
    const tagsDraft = tagging.replace(' value  ', ' edited  ')
    const lifecycleDraft = setLifecycleRuleStatus(lifecycle, 0, 'Enabled')
    editor.edit('tagging', tagsDraft)
    editor.edit('lifecycle', lifecycleDraft)
    await editor.write('lifecycle', { action: 'write', document: lifecycleDraft })
    expect(editor.state('lifecycle')).toMatchObject({ original: lifecycleDraft, draft: lifecycleDraft, notice: 'Saved.' })
    expect(editor.dirty.value).toBe(true)

    documents.set('tagging', 'denied')
    await editor.refreshLoaded()
    expect(editor.state('tagging')).toMatchObject({ original: tagging, draft: tagsDraft, reading: { status: 'error', message: 'Access denied' } })
    expect(editor.state('lifecycle').reading?.status).toBe('configured')
    editor.discard()
    expect(editor.state('tagging').draft).toBe(tagging)
    expect(editor.dirty.value).toBe(false)
  })

  it('finds a disabled lifecycle rule across buckets and keeps per-bucket failures', async () => {
    const results = new Map<string, ConfigurationReading>()
    const read = vi.fn(async (bucket: string) => {
      if (bucket === 'denied') throw new Error('Access denied')
      if (bucket === 'empty') throw { kind: 'missing' }
      return bucket === 'archive' ? lifecycle : setLifecycleRuleStatus(lifecycle, 0, 'Enabled')
    })
    await scanConfigurations(['regular', 'archive', 'denied', 'empty'], ['lifecycle'], read, (bucket, _, result) => results.set(bucket, result), () => true)
    const found = Array.from(results).flatMap(([bucket, result]) => result.status === 'configured'
      ? extractRules('lifecycle', result.document).filter(rule => rule.status === 'Disabled').map(rule => ({ bucket, ...rule }))
      : [])
    expect(found).toMatchObject([{ bucket: 'archive', name: 'Retire reports', detail: ' reports/', raw: expect.stringContaining('<e:Extra') }])
    expect(results.get('denied')).toMatchObject({ status: 'error', message: 'Access denied' })
    expect(results.get('empty')).toMatchObject({ status: 'missing' })
  })

  it('keeps review scoped to selected backup sections and leaves an unrelated draft intact', async () => {
    const targetDocument = tagging.replace(' value  ', ' target ')
    vi.spyOn(bucketsApi, 'importBackup').mockResolvedValue(JSON.stringify({
      format: 'storeforge-bucket-config', version: 1, bucket: 'source', documents: { tagging, lifecycle },
    }))
    vi.spyOn(bucketsApi, 'configure').mockImplementation(async (_, __, section) => section === 'tagging' ? targetDocument : '')
    const request = vi.fn(async (section: Configuration, change: ConfigurationChange) => change.action === 'write' ? '' : section === 'tagging' ? tagging : '')
    const editors = useBucketConfiguration(request)
    editors.edit('lifecycle', lifecycle)
    mount(BucketBackupDialog, { connection: 'fixture', bucket: 'target', mode: 'restore', editors })
    const dialog = document.querySelector('dialog')!
    await vi.waitFor(() => expect(dialog.textContent).toContain('Discard or save the local draft first'))
    const tag = Array.from(dialog.querySelectorAll<HTMLInputElement>('input')).find(input => input.value === 'tagging')!
    expect(tag.checked).toBe(false)
    expect(button(dialog, 'Restore selected').disabled).toBe(true)
    tag.click()
    await nextTick()
    button(dialog, 'Restore selected').click()
    await vi.waitFor(() => expect(dialog.textContent).toContain('Restored'))
    expect(request).toHaveBeenCalledWith('tagging', { action: 'write', document: tagging })
    expect(request.mock.calls.some(([section]) => section === 'lifecycle')).toBe(false)
    expect(editors.state('lifecycle').draft).toBe(lifecycle)
  })

  it('keeps a partial creation visible and retries only the pending versioning operation', async () => {
    const create = vi.spyOn(bucketsApi, 'create').mockResolvedValue({ versioningError: 'Versioning denied' })
    const finish = vi.spyOn(bucketsApi, 'finishVersioning').mockResolvedValue()
    const created = vi.fn()
    mount(NewBucketDialog, { profile, onCreated: created })
    const dialog = document.querySelector('dialog')!
    const inputs = dialog.querySelectorAll('input')
    inputs[0]!.value = 'archive-fixture'
    inputs[0]!.dispatchEvent(new Event('input', { bubbles: true }))
    inputs[1]!.value = 'zone-a:archive'
    inputs[1]!.dispatchEvent(new Event('input', { bubbles: true }))
    inputs[3]!.click()
    await nextTick()
    button(dialog, 'Create bucket').click()
    await vi.waitFor(() => expect(dialog.textContent).toContain('Bucket archive-fixture exists'))
    expect(created).not.toHaveBeenCalled()
    button(dialog, 'Retry versioning').click()
    await vi.waitFor(() => expect(created).toHaveBeenCalledWith('archive-fixture'))
    expect(create).toHaveBeenCalledExactlyOnceWith('fixture', 'archive-fixture', { location: 'zone-a:archive', objectLock: false, versioning: true })
    expect(finish).toHaveBeenCalledExactlyOnceWith('fixture', 'archive-fixture', 'zone-a:archive')
  })

  it('stages lifecycle quick actions before the provider can receive a write', async () => {
    const configure = vi.spyOn(bucketsApi, 'configure').mockImplementation(async (_, __, section) =>
      section === 'lifecycle' ? lifecycle : configurations.find(item => item.id === section)!.example)
    const root = mount(BucketSettings, { connection: 'fixture', bucket: 'archive', demo: false, managed: true })
    const nav = root.querySelector('nav')!
    nav.querySelectorAll('button').forEach(item => { if (item.textContent?.startsWith('Lifecycle rules')) item.click() })
    await vi.waitFor(() => expect(button(root, 'Enable').disabled).toBe(false))
    button(root, 'Enable').click()
    await vi.waitFor(() => expect(button(root, 'Disable').disabled).toBe(false))
    expect(configure.mock.calls.every(([, , , change]) => change.action === 'read')).toBe(true)
    expect(root.textContent).toContain('Draft')
  })
})
