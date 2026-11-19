// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, type Component } from 'vue'
import { createPinia } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import AdminWorkspace from '../src/features/administration/components/AdminWorkspace.vue'
import RgwInventory from '../src/features/administration/components/RgwInventory.vue'
import type { RgwAccount, RgwChange, RgwKey, RgwRow, RgwTarget, RgwUser } from '../src/features/administration/model/rgw'
import type { Connection } from '../src/features/connection/model/connection'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }))
const desktop = vi.mocked(invoke)
const cleanup: (() => void)[] = []
const blank = { target: null, issued: null, profile: null, issue: null }
const research = { tenant: 'research', uid: 'reader' }
const finance = { tenant: 'finance', uid: 'reader' }
const account: RgwTarget = { kind: 'account', id: 'RGW400' }
const profile: Connection = {
  id: 'gateway', name: 'Gateway', provider: 'ceph', endpoint: 'https://rgw.fixture',
  region: 'us-east-1', pathStyle: true, knownBuckets: [],
  services: { adminOps: { principalId: 'ops', endpoint: '', enabled: true, managedExternally: false, capability: 'detected' } },
}

interface TestCall {
  connection: string
  request: { action: string; kind?: string; managed?: boolean; marker?: string }
}

beforeEach(() => {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true,
    value(this: HTMLDialogElement) { this.open = true },
  })
})

afterEach(() => {
  cleanup.splice(0).reverse().forEach(callback => callback())
  desktop.mockReset()
})

function mount(component: Component, props: Record<string, unknown>): HTMLElement {
  const host = document.body.appendChild(document.createElement('div'))
  const application = createApp({ render: () => h(component, props) })
  application.use(createPinia())
  application.mount(host)
  cleanup.push(() => { application.unmount(); host.remove() })
  return host
}

function click(root: ParentNode, label: string): void {
  const button = Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find(element =>
    element.textContent?.trim() === label || element.getAttribute('aria-label') === label)
  if (!button) throw new Error(`Missing ${label}`)
  expect(button.disabled).toBe(false)
  button.click()
}

function field(root: ParentNode, label: string): HTMLInputElement {
  const element = Array.from(root.querySelectorAll('label')).find(element => element.textContent?.trim() === label)
  const input = element?.querySelector('input')
  if (!input) throw new Error(`Missing field ${label}`)
  return input
}

function type(input: HTMLInputElement, value: string): void {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function row(target: RgwTarget, managed = false): RgwRow {
  return { target, managed, root: null }
}

function accessKey(id: string, active: boolean): RgwKey {
  return { id, active, metadata: { name: '', notes: '', expiresAt: null }, enforcement: null, privateProfile: null, issue: null, providerMissing: false }
}

function user(overrides: Partial<RgwUser> = {}): RgwUser {
  return {
    identity: research, displayName: 'Reader', email: '', account: null, accountRoot: false, suspended: false,
    maxBuckets: 10, quota: { enabled: true, maxBytes: 512, maxObjects: 20 }, bucketQuota: null, usage: null,
    keys: [], connectionKeyId: null, ...overrides,
  }
}

describe('RGW identity workspaces', () => {
  it('imports equal UIDs across pages while retaining their exact tenants', async () => {
    const targets: RgwTarget[] = [{ kind: 'user', identity: research }, { kind: 'user', identity: finance }]
    let managed: RgwRow[] = []
    desktop.mockImplementation(async (command, args) => {
      const { connection, request } = args as unknown as TestCall
      expect(connection).toBe(profile.id)
      if (command === 'change_rgw_identity') {
        expect(request).toEqual({ action: 'importUsers', identities: [research, finance] })
        managed = targets.map(target => row(target, true))
        return blank
      }
      expect(request.action).toBe('list')
      if (request.managed) return { items: managed, marker: null }
      return request.marker ? { items: [row(targets[1]!)], marker: null } : { items: [row(targets[0]!)], marker: 'next' }
    })
    const host = mount(RgwInventory, { connection: profile.id, kind: 'user', managed: true })
    click(host, 'Import users')
    await vi.waitFor(() => field(document, 'research$reader').click())
    await vi.waitFor(() => click(document, 'Next page'))
    await vi.waitFor(() => field(document, 'finance$reader').click())
    await vi.waitFor(() => click(document, 'Import 2 selected'))
    await vi.waitFor(() => {
      expect(host.querySelector('[aria-label="Open research$reader"]')).toBeTruthy()
      expect(host.querySelector('[aria-label="Open finance$reader"]')).toBeTruthy()
    })
  })

  it('uses root discovery when no override is supplied and displays the registered root', async () => {
    const root = { tenant: 'research', uid: 'nonstandard-root' }
    const provider: RgwAccount = {
      id: account.id, tenant: root.tenant, name: 'Research', email: '',
      limits: { maxUsers: 25, maxBuckets: 100, maxRoles: 40, maxGroups: 20, maxAccessKeys: 2 },
      quota: null, bucketQuota: null, usage: null, userCount: null, root,
    }
    let registered = false
    desktop.mockImplementation(async (command, args) => {
      const { request } = args as unknown as TestCall
      if (command === 'change_rgw_identity') {
        expect(request).toEqual({ action: 'importAccounts', entries: [{ id: account.id, root: null }] })
        registered = true
        return blank
      }
      if (request.action === 'account') return provider
      if (request.managed && !registered) return { items: [], marker: null }
      return { items: [{ target: account, managed: registered, root: registered ? root : null }], marker: null }
    })
    const host = mount(RgwInventory, { connection: profile.id, kind: 'account', managed: true })
    click(host, 'Import accounts')
    await vi.waitFor(() => field(document, account.id).click())
    await vi.waitFor(() => click(document, 'Import 1 selected'))
    await vi.waitFor(() => click(host, `Open ${account.id}`))
    await vi.waitFor(() => {
      expect(host.textContent).toContain('research$nonstandard-root')
      expect(host.textContent).toContain('Keys per user')
    })
  })

  it('keeps an owned UID import in review and sends only the selected quota fields', async () => {
    const identity = { tenant: 'research', uid: 'root' }
    const target = { kind: 'user', identity } as const
    let detail = user({ identity, account: account.id, accountRoot: true })
    desktop.mockImplementation(async (command, args) => {
      const { request } = args as unknown as TestCall
      if (command === 'change_rgw_identity') {
        const change = request as unknown as RgwChange
        if (change.action === 'importUsers') {
          throw { kind: 'invalid', message: 'research$root is owned by an RGW account.' }
        }
        expect(change).toEqual({ action: 'setQuota', target, bucket: false, quota: { enabled: true, maxBytes: 1024, maxObjects: 20 } })
        detail = { ...detail, quota: { enabled: true, maxBytes: 1024, maxObjects: 20 } }
        return blank
      }
      if (request.action === 'user') return detail
      return { items: request.managed ? [] : [row(target)], marker: null }
    })
    const managed = mount(RgwInventory, { connection: profile.id, kind: 'user', managed: true })
    click(managed, 'Import users')
    await vi.waitFor(() => field(document, 'research$root').click())
    await vi.waitFor(() => click(document, 'Import 1 selected'))
    await vi.waitFor(() => expect(document.body.textContent).toContain('is owned by an RGW account'))
    click(document, 'Cancel')
    const cluster = mount(RgwInventory, { connection: profile.id, kind: 'user', managed: false })
    await vi.waitFor(() => click(cluster, 'Open research$root'))
    await vi.waitFor(() => click(cluster, 'Edit user quota'))
    await vi.waitFor(() => type(field(document, 'Maximum bytes'), '1024'))
    click(document, 'Save quota')
    await vi.waitFor(() => {
      expect(cluster.textContent).toMatch(/Account root\s*Yes/)
      expect(cluster.textContent).toContain(`${(1024).toLocaleString()} bytes`)
    })
  })

  it('shows delegated key ownership, one-time issuance, and old and new provider states after refresh', async () => {
    let keys = [accessKey('old-key', true)]
    desktop.mockImplementation(async (command, args) => {
      const { request } = args as unknown as TestCall
      if (command === 'read_rgw_identity') {
        expect(request.action).toBe('delegatedKeys')
        return user({ keys, connectionKeyId: 'old-key' })
      }
      expect(command).toBe('change_rgw_identity')
      const change = request as unknown as Extract<RgwChange, { action: 'key' }>
      expect(change.scope).toBe('delegated')
      expect(change.owner).toEqual(research)
      if (change.change.action === 'create') {
        keys = [...keys, accessKey('new-key', true)]
        return { ...blank, issued: { accessKey: 'new-key', secretKey: 'one-time-secret' } }
      }
      expect(change.change).toEqual({ action: 'status', id: 'new-key', active: false })
      keys = [accessKey('old-key', true), accessKey('new-key', false)]
      return blank
    })
    const host = mount(AdminWorkspace, { connection: profile, scope: 's3', demo: false })
    click(host, 'Delegated RGW keys')
    await vi.waitFor(() => click(host, 'Create RGW key'))
    await vi.waitFor(() => click(document, 'Issue key'))
    await vi.waitFor(() => expect(field(document, 'Secret access key').value).toBe('one-time-secret'))
    click(document, 'Done')
    await vi.waitFor(() => click(host, 'Deactivate'))
    await vi.waitFor(() => click(document, 'Deactivate key'))
    await vi.waitFor(() => {
      const refreshed = Array.from(host.querySelectorAll('.rgw-key-row')).map(element => element.textContent)
      expect(refreshed[0]).toMatch(/old-key.*Active/)
      expect(refreshed[1]).toMatch(/new-key.*Inactive/)
      expect(document.body.querySelector('input[value="one-time-secret"]')).toBeNull()
    })
  })
})
