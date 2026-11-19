// @vitest-environment jsdom

import { invoke } from '@tauri-apps/api/core'
import { createApp, h } from 'vue'
import { afterEach, expect, it, vi } from 'vitest'
import MigrationWorkspace from '../src/features/transfer/components/MigrationWorkspace.vue'
import type { Migration, MigrationRun, MigrationRunStatus } from '../src/features/transfer/model/migration'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }))
const desktop = vi.mocked(invoke)
const mounted: (() => void)[] = []
const mapping = { source: 'archive', target: 'archive-new' }
const updatedAt = new Date().toISOString()

afterEach(() => {
  mounted.splice(0).forEach(dispose => dispose())
  desktop.mockReset()
})

function mount(): HTMLElement {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value(this: HTMLDialogElement) { this.open = true } })
  const host = document.body.appendChild(document.createElement('div'))
  const app = createApp({ render: () => h(MigrationWorkspace, { connection: 'source', buckets: [], profiles: [], demo: false }) })
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

function type(phrase: string): void {
  const input = document.querySelector<HTMLInputElement>('dialog input')
  if (!input) throw new Error('Missing confirmation field')
  input.value = phrase
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function migration(status: MigrationRunStatus, outcome: Partial<MigrationRun['buckets'][number]> = {}): Migration {
  return {
    id: 'migration', revision: 'revision', updatedAt, report: null, checking: null, recoveryRequired: false,
    recoveryError: null, canRestore: outcome.sourceProtected ?? false,
    draft: {
      source: 'source', target: 'target', mappings: [mapping], mode: 'preCopy', method: 'streamed', history: true,
      settings: false, parallelism: 4, protectDestination: true, grantSourceRead: false,
    },
    run: {
      status, buckets: [{
        mapping, status: 'awaitingCutover', completed: 3, total: 3, owned: true,
        currentKey: null, error: null, cleanupError: null, sourceDeleted: false, sourceProtected: false, ...outcome,
      }],
    },
  }
}

it('requires cutover confirmation and exposes a failed bucket for retry', async () => {
  desktop.mockImplementation(async (command, args) => {
    if (command === 'list_migrations') return [migration('awaitingCutover')]
    if (command === 'migration_action' && args?.action === 'cutover') {
      return migration('failed', { status: 'failed', error: 'Exact version could not be read.' })
    }
    if (command === 'migration_action' && args?.action === 'retry') {
      return migration('verified', { status: 'verified', sourceProtected: true })
    }
    throw new Error(command)
  })
  const host = mount()
  await vi.waitFor(() => click(host, 'Open migration'))
  await vi.waitFor(() => click(host, 'Review cutover'))
  await vi.waitFor(() => expect(button(document, 'Start cutover').disabled).toBe(true))
  type('CUTOVER')
  await vi.waitFor(() => click(document, 'Start cutover'))
  await vi.waitFor(() => {
    expect(host.textContent).toContain('Exact version could not be read.')
    click(host, 'Retry failed buckets')
  })
  await vi.waitFor(() => expect(host.textContent).toContain('Source access changes recorded'))
  expect(desktop).toHaveBeenCalledWith('migration_action', {
    id: 'migration', revision: 'revision', action: 'cutover', confirmation: 'CUTOVER',
  })
  expect(desktop).toHaveBeenCalledWith('migration_action', {
    id: 'migration', revision: 'revision', action: 'retry', confirmation: null,
  })
})

it('confirms exact source deletion, preserves verified copy on failure, and restores access', async () => {
  desktop.mockImplementation(async (command, args) => {
    if (command === 'list_migrations') return [migration('verified', { status: 'verified', sourceProtected: true })]
    if (command === 'migration_action' && args?.action === 'deleteSources') {
      return migration('verified', { status: 'verified', sourceProtected: true, cleanupError: 'Fresh destination content differs.' })
    }
    if (command === 'restore_migration_access') return migration('verified', { status: 'verified' })
    throw new Error(command)
  })
  const host = mount()
  await vi.waitFor(() => click(host, 'Open migration'))
  await vi.waitFor(() => click(host, 'Delete source buckets'))
  const dialog = await vi.waitFor(() => {
    const match = document.querySelector('dialog')
    if (!match) throw new Error('Missing confirmation dialog')
    return match
  })
  type('DELETE archive-new')
  await vi.waitFor(() => expect(button(dialog, 'Delete source buckets').disabled).toBe(true))
  type('DELETE archive')
  await vi.waitFor(() => click(dialog, 'Delete source buckets'))
  await vi.waitFor(() => {
    expect(host.textContent).toContain('Fresh destination content differs.')
    click(host, 'Restore access')
  })
  await vi.waitFor(() => {
    expect(host.textContent).toContain('Verified')
    expect(host.textContent).not.toContain('Delete source buckets')
  })
  expect(desktop).toHaveBeenCalledWith('migration_action', {
    id: 'migration', revision: 'revision', action: 'deleteSources', confirmation: 'DELETE archive',
  })
})
