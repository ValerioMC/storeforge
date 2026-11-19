// @vitest-environment jsdom

import { invoke } from '@tauri-apps/api/core'
import { createPinia } from 'pinia'
import { afterEach, expect, it, vi } from 'vitest'
import { createApp, h } from 'vue'
import ConnectionEditor from '../src/features/connection/components/ConnectionEditor.vue'
import { endpointSettings, type Connection } from '../src/features/connection/model/connection'
import { parseProfile } from '../src/features/connection/model/profile-import'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }))
const desktop = vi.mocked(invoke)
const mounted: (() => void)[] = []
const fields = {
  name: 'Gateway', provider: 'ceph' as const, endpoint: 'https://storage.example.test',
  region: 'us-east-1', pathStyle: true, knownBuckets: [],
}
const proxyUrl = 'http://proxy.example.test:33128'

afterEach(() => {
  mounted.splice(0).forEach(dispose => dispose())
  desktop.mockReset()
})

function click(label: string): void {
  const button = [...document.querySelectorAll<HTMLButtonElement>('dialog button')].find(button => button.textContent?.trim() === label)
  if (!button) throw new Error(`Missing button "${label}"`)
  button.click()
}

it('imports the per-connection proxy and preserves legacy profile defaults', () => {
  expect(parseProfile(JSON.stringify({ ...fields, settings: { proxyUrl } })).settings.proxyUrl).toBe(proxyUrl)
  expect(parseProfile(JSON.stringify(fields)).settings.proxyUrl).toBe('')
  expect(() => parseProfile(JSON.stringify({ ...fields, settings: { proxyUrl: true } }))).toThrow('Invalid endpoint settings.')
})

it('checks and saves the proxy edited on a single connection', async () => {
  desktop.mockImplementation(async (command, args) => {
    if (command === 'test_connection') return { capabilities: { s3: 'detected' }, bucketCount: 1 }
    if (command === 'save_connection' && args && 'draft' in args) return args.draft
    throw new Error(command)
  })
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value(this: HTMLDialogElement) { this.open = true } })
  const profile: Connection = { ...fields, id: 'gateway', settings: endpointSettings() }
  const host = document.body.appendChild(document.createElement('div'))
  const app = createApp({ render: () => h(ConnectionEditor, { profile }) })
  app.use(createPinia()).mount(host)
  mounted.push(() => { app.unmount(); host.remove() })
  const field = document.querySelector<HTMLInputElement>('input[placeholder="http://proxy.example.com:33128"]')
  if (!field) throw new Error('Missing proxy field')
  field.value = proxyUrl
  field.dispatchEvent(new Event('input', { bubbles: true }))
  click('Continue')
  await vi.waitFor(() => click('Continue'))
  await vi.waitFor(() => click('Check credentials'))
  await vi.waitFor(() => expect(document.body.textContent).toContain('1 bucket available'))
  click('Review setup')
  await vi.waitFor(() => {
    expect(document.body.textContent).toContain(proxyUrl)
    click('Apply connection')
  })
  const draft = expect.objectContaining({ id: 'gateway', settings: expect.objectContaining({ proxyUrl }) })
  expect(desktop).toHaveBeenCalledWith('test_connection', { draft })
  expect(desktop).toHaveBeenCalledWith('save_connection', { draft })
})
