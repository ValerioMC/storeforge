// @vitest-environment jsdom

import { invoke } from '@tauri-apps/api/core'
import { createPinia } from 'pinia'
import { afterEach, expect, it, vi } from 'vitest'
import { createApp, h } from 'vue'
import BucketMaintenance from '../src/features/transfer/components/BucketMaintenance.vue'
import type { BucketResult, MaintenanceKind, MaintenanceReport, PurgePreview, PurgeRequest } from '../src/features/transfer/model/maintenance'
import type { Job } from '../src/features/transfer/model/transfer'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }))
const desktop = vi.mocked(invoke)
const cleanup: (() => void)[] = []
const scopes = ['blocked', 'archive', 'completed'].map(bucket => ({ bucket, prefix: '' }))
const counts = { current: 2, history: 3, markers: 1 }
const empty = { current: 0, history: 0, markers: 0 }

afterEach(() => {
  cleanup.splice(0).forEach(dispose => dispose())
  desktop.mockReset()
})

function mount(kind: MaintenanceKind): HTMLElement {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value(this: HTMLDialogElement) { this.open = true } })
  const host = document.body.appendChild(document.createElement('div'))
  const app = createApp({ render: () => h(BucketMaintenance, {
    kind, location: { connection: 'gateway', bucket: 'blocked', prefix: '' },
    sourceBuckets: scopes.map(({ bucket }) => ({ name: bucket, createdAt: null })), demo: false,
  }) })
  app.use(createPinia()).mount(host)
  cleanup.push(() => { app.unmount(); host.remove() })
  return host
}

function getButton(root: ParentNode, label: string): HTMLButtonElement {
  const button = [...root.querySelectorAll<HTMLButtonElement>('button')].find(node => node.textContent?.trim() === label)
  if (!button) throw new Error(`Missing ${label}`)
  return button
}

function click(root: ParentNode, label: string): void {
  const button = getButton(root, label)
  expect(button.disabled).toBe(false)
  button.click()
}

function toggle(root: HTMLElement, label: string): void {
  const row = [...root.querySelectorAll('label')].find(node => node.textContent?.trim() === label)
  const input = row?.querySelector('input')
  if (!input) throw new Error(`Missing checkbox ${label}`)
  input.click()
}

function input(element: HTMLInputElement, value: string): void {
  element.value = value
  element.dispatchEvent(new Event('input', { bubbles: true }))
}

function outcome(kind: MaintenanceKind, buckets: BucketResult[]): Job {
  const scope = { connection: 'gateway', buckets, parallelism: 4, includeVersions: true }
  const maintenance: MaintenanceReport = kind === 'purge'
    ? { ...scope, kind, removeBucket: true }
    : { ...scope, kind, since: null, method: { kind: 'head' } }
  return {
    id: 'operation', label: 'Bucket operation', status: 'failed', startedAt: '2025-01-01T00:00:00Z', finishedAt: null,
    total: 3, succeeded: 1, failed: 2, failures: [], current: null, cancelRequested: false,
    maintenance,
  }
}

function results(): BucketResult[] {
  return [
    { scope: scopes[0]!, status: 'failed', planned: empty, completed: empty, failed: 1, removed: false,
      failures: [{ stage: 'listing', key: null, version: null, message: 'Access denied' }] },
    { scope: scopes[1]!, status: 'failed', planned: counts, completed: { ...empty, history: 1 }, failed: 1, removed: false,
      failures: [{ stage: 'delete', key: ' retained ', version: 'old-version', message: 'Deletion refused' }] },
    { scope: scopes[2]!, status: 'completed', planned: counts, completed: counts, failed: 0, removed: true, failures: [] },
  ]
}

function preview(request: PurgeRequest): PurgePreview {
  const buckets = request.selection.targets.map(scope => ({
    scope, counts: scope.bucket === 'blocked' ? null : counts, historyError: null,
    error: scope.bucket === 'blocked' ? 'Access denied' : null,
  }))
  return { request, revision: 'profile-revision', buckets,
    confirmation: buckets.filter(bucket => bucket.counts).map(bucket => bucket.scope.bucket).join(', ') }
}

it('previews mixed purge results, requires the exact eligible phrase, and retries only failed buckets', async () => {
  let started = false
  desktop.mockImplementation(async (command, args) => {
    if (command === 'preview_bucket_purge') return preview(args?.request as PurgeRequest)
    if (command === 'start_transfer') { started = true; return 'operation' }
    if (command === 'list_jobs') return started ? [outcome('purge', results())] : []
    throw new Error(command)
  })
  const host = mount('purge')
  click(host, 'Select visible')
  toggle(host, 'Permanently remove all versions and delete markers')
  await vi.waitFor(() => toggle(host, 'Delete each bucket after a successful purge'))
  click(host, 'Preview purge')
  await vi.waitFor(() => {
    expect(host.textContent).toContain('Access denied')
    click(host, 'Review purge confirmation')
  })
  const dialog = await vi.waitFor(() => {
    const dialog = document.querySelector('dialog')
    if (!dialog) throw new Error('Missing confirmation')
    return dialog
  })
  const field = dialog.querySelector('input')!
  input(field, 'blocked, archive, completed')
  await vi.waitFor(() => expect(getButton(dialog, 'Empty and delete buckets').disabled).toBe(true))
  input(field, 'archive, completed')
  await vi.waitFor(() => click(dialog, 'Empty and delete buckets'))
  await vi.waitFor(() => {
    expect(desktop).toHaveBeenCalledWith('start_transfer', expect.objectContaining({
      transfer: expect.objectContaining({ kind: 'purge', confirmation: 'archive, completed' }),
    }))
    expect(host.textContent).toContain('old-version')
    expect(host.textContent).toContain('Preserved')
  })
  toggle(host, 'Delete each bucket after a successful purge')
  click(host, 'Preview failed buckets')
  await vi.waitFor(() => expect(desktop).toHaveBeenLastCalledWith('preview_bucket_purge', {
    request: { selection: { connection: 'gateway', parallelism: 4, targets: scopes.slice(0, 2) }, includeVersions: true, removeBucket: true },
  }))
})

it('starts a version-aware integrity job and filters bucket failures', async () => {
  const buckets = results()
  buckets[1]!.failures[0]!.stage = 'get'
  desktop.mockImplementation(async command => {
    if (command === 'check_integrity') return 'operation'
    if (command === 'list_jobs') return [outcome('integrity', buckets)]
    throw new Error(command)
  })
  const host = mount('integrity')
  click(host, 'Select visible')
  toggle(host, 'Check all object versions')
  click(host, 'Run integrity check')
  await vi.waitFor(() => {
    expect(desktop).toHaveBeenCalledWith('check_integrity', { request: {
      selection: { connection: 'gateway', parallelism: 4, targets: scopes },
      method: { kind: 'head' }, since: null, includeVersions: true,
    } })
    expect(host.textContent).toContain('Access denied')
  })
  const filter = host.querySelector<HTMLInputElement>('[placeholder="Key, version or error"]')!
  input(filter, 'old-version')
  await vi.waitFor(() => {
    expect(host.textContent).toContain('old-version')
    expect(host.textContent).not.toContain('Access denied')
  })
})
