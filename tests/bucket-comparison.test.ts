// @vitest-environment jsdom

import { invoke } from '@tauri-apps/api/core'
import { createPinia } from 'pinia'
import { afterEach, expect, it, vi } from 'vitest'
import { createApp, h } from 'vue'
import BucketComparison from '../src/features/transfer/components/BucketComparison.vue'
import type { Connection } from '../src/features/connection/model/connection'
import type { BucketComparison as Report, ObjectDifference } from '../src/features/transfer/model/bucket-comparison'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }))
const desktop = vi.mocked(invoke)
const cleanup: (() => void)[] = []
const profile: Connection = { id: 'left', name: 'Source profile', provider: 'ceph', endpoint: 'https://source.example', region: 'us-east-1', pathStyle: true, knownBuckets: [] }
const destination: Connection = { ...profile, id: 'right', name: 'Destination profile', endpoint: 'https://destination.example' }

afterEach(() => {
  cleanup.splice(0).forEach(dispose => dispose())
  desktop.mockReset()
})

function click(root: ParentNode, label: string): void {
  const button = [...root.querySelectorAll<HTMLButtonElement>('button')].find(node =>
    node.textContent?.trim() === label || node.getAttribute('aria-label') === label)
  if (!button) throw new Error(`Missing button ${label}`)
  expect(button.disabled).toBe(false)
  button.click()
}

function mount(): HTMLElement {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value(this: HTMLDialogElement) { this.open = true } })
  const host = document.body.appendChild(document.createElement('div'))
  const app = createApp({ render: () => h(BucketComparison, {
    location: { connection: 'left', bucket: 'logs', prefix: '' },
    profiles: [profile, destination], sourceBuckets: [{ name: 'logs', createdAt: null }, { name: 'media', createdAt: null }], demo: false,
  }) })
  app.use(createPinia()).mount(host)
  cleanup.push(() => { app.unmount(); host.remove() })
  return host
}

function selectDestination(host: HTMLElement): void {
  const labels = [...host.querySelectorAll('label')]
  const destination = labels.find(label => label.textContent?.includes('Destination connection'))
  const trigger = destination?.querySelector('button')
  if (!trigger) throw new Error('Missing destination picker')
  trigger.click()
}

it('matches bucket names and sends just the explicitly reviewed whitespace-sensitive key', async () => {
  const row: ObjectDifference = { key: ' logs/report ', source: { size: 12, etag: '"multipart-2"', modified: '2025-01-01T00:00:00Z' }, target: null, evidence: 'sourceOnly', matchingEtag: null, issue: null }
  const report: Report = {
    scope: { sourceConnection: 'left', targetConnection: 'right', sourceRevision: null, targetRevision: null },
    reports: [{
      pair: { source: 'logs', target: 'logs' }, contentError: null, configuration: [],
      contents: { sourceCount: 2, targetCount: 1, matchingSize: 0, matchingDigest: 0, differentSize: 0, differentDigest: 0, inconclusive: 1, sourceOnly: 1, targetOnly: 0, afterCutoff: 0, unknownModified: 0, hidden: 1, details: [row] },
    }],
  }
  desktop.mockImplementation(async command => {
    if (command === 'list_buckets') return [{ name: 'logs', createdAt: null }, { name: 'media', createdAt: null }]
    if (command === 'compare_buckets') return report
    if (command === 'remediate_bucket_difference') return undefined
    throw new Error(command)
  })
  const host = mount()
  selectDestination(host)
  await vi.waitFor(() => {
    const option = [...document.querySelectorAll<HTMLElement>('[role=option]')].find(node => node.textContent === 'Destination profile')
    if (!option) throw new Error('Missing destination profile')
    option.click()
  })
  await vi.waitFor(() => click(host, 'Match by bucket name'))
  await vi.waitFor(() => click(host, 'Compare buckets'))
  await vi.waitFor(() => expect(desktop).toHaveBeenCalledWith('compare_buckets', {
    source: 'left', target: 'right', request: {
      pairs: [{ source: 'logs', target: 'logs' }, { source: 'media', target: 'media' }],
      content: 'etag', configuration: [], modifiedBefore: null,
    },
  }))
  await vi.waitFor(() => click(host, 'Review object change'))
  await vi.waitFor(() => {
    expect(document.querySelector('dialog')?.textContent).toContain('" logs/report "')
    expect(desktop.mock.calls.filter(([command]) => command === 'remediate_bucket_difference')).toHaveLength(0)
    click(document, 'Copy object to destination')
  })
  await vi.waitFor(() => expect(desktop).toHaveBeenCalledWith('remediate_bucket_difference', {
    scope: report.scope, pair: report.reports[0]?.pair,
    action: { kind: 'object', key: row.key, source: row.source, target: null },
  }))
})
