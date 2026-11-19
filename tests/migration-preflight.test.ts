// @vitest-environment jsdom

import { invoke } from '@tauri-apps/api/core'
import { createApp, h } from 'vue'
import { afterEach, expect, it, vi } from 'vitest'
import MigrationWorkspace from '../src/features/transfer/components/MigrationWorkspace.vue'
import type { Migration, MigrationDraft, MigrationReport } from '../src/features/transfer/model/migration'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }))
const desktop = vi.mocked(invoke)
const mounted: (() => void)[] = []
const draft: MigrationDraft = {
  source: 'source', target: 'target', mappings: [{ source: 'archive', target: 'archive-new' }],
  mode: 'preCopy', method: 'streamed', history: true, settings: true, parallelism: 4,
  protectDestination: false, grantSourceRead: false,
}

afterEach(() => {
  mounted.splice(0).forEach(dispose => dispose())
  desktop.mockReset()
})

function mount(): HTMLElement {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value(this: HTMLDialogElement) { this.open = true } })
  const host = document.body.appendChild(document.createElement('div'))
  const app = createApp({ render: () => h(MigrationWorkspace, {
    connection: 'source', buckets: [{ name: 'archive', createdAt: null }], demo: false,
    profiles: ['source', 'target'].map(id => ({ id, name: id, provider: 'compatible' as const, endpoint: '', region: '', pathStyle: true, knownBuckets: [] })),
  }) })
  app.mount(host)
  mounted.push(() => { app.unmount(); host.remove() })
  return host
}

function button(root: ParentNode, label: string): HTMLButtonElement {
  const match = [...root.querySelectorAll('button')].find(item => item.textContent?.trim() === label)
  if (!match) throw new Error(`Missing button "${label}"`)
  return match
}

function click(root: ParentNode, label: string): void {
  const match = button(root, label)
  expect(match.disabled).toBe(false)
  match.click()
}

function report(phase: MigrationReport['phase'], expiresAt: number, issues: string[] = []): MigrationReport {
  return {
    revision: 'rev-1', phase, checkedAt: new Date(Date.now()).toISOString(), expiresAt: new Date(expiresAt).toISOString(),
    pairs: [{ mapping: draft.mappings[0]!, sample: ' exact ', settingsCopied: ['versioning'], settingsOmitted: ['acl'], issues }],
  }
}

function record(configuration: Partial<Migration>): Migration {
  return {
    id: 'migration', revision: 'rev-1', draft, updatedAt: new Date(Date.now()).toISOString(), report: null,
    checking: null, run: null, canRestore: false, recoveryRequired: false, recoveryError: null, ...configuration,
  }
}

it('reopens a persisted draft, exposes expired checks, and invalidates the report after edit', async () => {
  desktop.mockImplementation(async (command, args) => {
    if (command === 'list_migrations') return [record({ report: report('active', Date.now() - 5000) })]
    if (command === 'save_migration') {
      return record({ revision: 'rev-2', draft: (args && 'draft' in args ? args.draft : undefined) as MigrationDraft })
    }
    throw new Error(command)
  })
  const host = mount()
  await vi.waitFor(() => click(host, 'Open migration'))
  await vi.waitFor(() => {
    expect(host.textContent).toContain('Checks expired')
    click(host, 'Edit draft')
  })
  await vi.waitFor(() => {
    const name = host.querySelector<HTMLInputElement>('input[placeholder="New bucket name"]')
    if (!name) throw new Error('Missing destination name')
    name.value = 'archive-next'
    name.dispatchEvent(new Event('input', { bubbles: true }))
    click(host, 'Save draft')
  })
  await vi.waitFor(() => {
    expect(host.textContent).toContain('archive-next')
    expect(host.textContent).not.toContain('Checks expired')
  })
  expect(desktop).toHaveBeenCalledWith('save_migration', {
    id: 'migration', revision: 'rev-1', draft: { ...draft, mappings: [{ source: 'archive', target: 'archive-next' }] },
  })
})

it('confirms active checks separately and requires policy recovery after a failed restoration', async () => {
  desktop.mockImplementation(async (command, args) => {
    if (command === 'list_migrations') return [record({ report: report('read', Date.now() + 60_000) })]
    if (command === 'check_migration' && args && 'phase' in args && args.phase === 'active') return record({
      recoveryRequired: true, canRestore: true, recoveryError: 'The provider declined the restoration.',
      report: report('active', Date.now() + 60_000, ['Temporary access requires recovery.']),
    })
    if (command === 'restore_migration_access') return record({})
    throw new Error(command)
  })
  const host = mount()
  await vi.waitFor(() => click(host, 'Open migration'))
  await vi.waitFor(() => click(host, 'Review active checks'))
  expect(desktop).not.toHaveBeenCalledWith('check_migration', expect.anything())
  await vi.waitFor(() => click(document, 'Run active checks'))
  await vi.waitFor(() => {
    expect(host.textContent).toContain('The provider declined the restoration.')
    expect(host.textContent).not.toContain('Edit draft')
    click(host, 'Restore access')
  })
  await vi.waitFor(() => expect(button(host, 'Edit draft').disabled).toBe(false))
  expect(desktop).toHaveBeenCalledWith('check_migration', {
    id: 'migration', revision: 'rev-1', phase: 'active', approved: true,
  })
  expect(desktop).toHaveBeenCalledWith('restore_migration_access', { id: 'migration', revision: 'rev-1' })
})
