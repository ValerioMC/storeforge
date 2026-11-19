// @vitest-environment jsdom
import { createPinia } from 'pinia'
import { createApp, nextTick, type App } from 'vue'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import OpenStackConnections from '../src/features/connection/components/OpenStackConnections.vue'
import { endpointSettings, type Connection } from '../src/features/connection/model/connection'
import type { OpenStackCloudStatus, OpenStackProjectAccess, OpenStackSession } from '../src/features/connection/model/openstack'
import { workspaceAvailable } from '../src/features/connection/model/workspace'
import { useConnections } from '../src/features/connection/store/connections'

const backend = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({
  desktop: backend.desktop, isTauri: () => true,
  errorMessage: (error: { message?: string }) => error.message ?? String(error),
}))

const cloud = {
  id: 'cloud', name: 'Research Cloud', authUrl: 'https://identity.example.org/v3',
  defaultDomain: 'Default', managedEndpointsOnly: true,
}
const session: OpenStackSession = {
  identity: 'Keystone Operator', expiresAt: '2099-07-15T12:00:00Z',
  projects: [{ id: 'build', name: 'Build', domainId: null }, { id: 'research', name: 'Research', domainId: 'default' }],
}
const access: OpenStackProjectAccess = {
  id: 'discovery', accessKeys: ['key-one', 'key-two'],
  targets: [
    { id: 'rome-public', region: 'Rome', interface: 'public', endpoint: 'https://rome.example.org/s3', managed: true, allowed: true },
    { id: 'rome-internal', region: 'Rome', interface: 'internal', endpoint: 'https://rome-internal.example.org', managed: true, allowed: true },
    { id: 'paris', region: 'Paris', interface: 'public', endpoint: 'https://paris.example.org', managed: false, allowed: false },
  ],
}
const direct: Connection = {
  id: 'direct', name: 'Direct S3', provider: 'compatible', endpoint: 'https://direct.example.org', region: 'Rome',
  pathStyle: true, knownBuckets: [],
}
const project: Connection = {
  ...direct, id: 'project', name: 'Research Cloud / Research / Rome', endpoint: 'https://rome-internal.example.org',
  settings: { ...endpointSettings(), openstack: { cloudId: cloud.id, projectName: 'Research', expiresAt: session.expiresAt } },
}
const browse = vi.fn()
let app: App<Element> | undefined
let host: HTMLDivElement
let status: OpenStackCloudStatus[]
let listedProfiles: Connection[]
let connections: ReturnType<typeof useConnections>
let activationFailure: { kind: string; message: string } | undefined

beforeEach(() => {
  const storage = new Map<string, string>()
  Object.defineProperty(window, 'localStorage', { configurable: true, value: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  } })
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value: function (this: HTMLDialogElement) {
    this.open = true
  } })
  status = [{ cloud, session: null, reconnectRequired: false }]
  listedProfiles = [direct]
  activationFailure = undefined
  browse.mockReset()
  backend.desktop.mockReset().mockImplementation(async (command: string, payload?: { forget?: boolean }) => {
    switch (command) {
      case 'list_openstack_clouds': return status
      case 'list_connections': return listedProfiles
      case 'authenticate_openstack':
        status = [{ cloud, session, reconnectRequired: false }]
        return session
      case 'discover_openstack_project': return access
      case 'activate_openstack_project':
        if (activationFailure) {
          status = [{ cloud, session: null, reconnectRequired: true }]
          throw activationFailure
        }
        listedProfiles = [direct, project]
        return project
      case 'disconnect_openstack_cloud':
        status = payload?.forget ? [] : [{ cloud, session: null, reconnectRequired: false }]
        listedProfiles = [direct]
        return
      default: throw new Error(`Unexpected command ${command}`)
    }
  })
  host = document.createElement('div')
  document.body.append(host)
})

afterEach(() => {
  app?.unmount()
  app = undefined
  host.remove()
})

async function mount(): Promise<void> {
  const pinia = createPinia()
  connections = useConnections(pinia)
  await connections.load()
  connections.select(direct.id)
  app = createApp(OpenStackConnections, { onBrowse: browse })
  app.use(pinia).mount(host)
}

function button(label: string): HTMLButtonElement {
  const element = Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(button =>
    (button.textContent?.trim() === label || button.getAttribute('aria-label') === label) && !button.closest('[inert]'))
  if (!element) throw new Error(`Expected button ${label}`)
  return element
}

function fill(label: string, value: string): void {
  const field = Array.from(document.querySelectorAll('label')).find(field => field.textContent?.trim() === label && !field.closest('[inert]'))
  const input = field?.querySelector('input')
  if (!input) throw new Error(`Expected input ${label}`)
  input.value = value
  input.dispatchEvent(new Event('input'))
}

async function select(label: string, option: string): Promise<void> {
  const select = button(label)
  select.click()
  await nextTick()
  const item = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="option"]'))
    .find(item => item.textContent?.trim() === option)
  if (!item) throw new Error(`Expected option ${option}`)
  item.click()
  await nextTick()
}

async function signInWithToken(): Promise<void> {
  await expect.poll(() => host.textContent).toContain('Sign in to Keystone')
  await select('Keystone authentication', 'Primary Keystone token')
  fill('Primary token', 'primary')
  await nextTick()
  button('Sign in to Keystone').click()
  await expect.poll(() => host.textContent).toContain('Keystone Operator')
}

async function selectProjectRegionAndKey(): Promise<void> {
  await select('OpenStack project', 'Research (research)')
  await expect.poll(() => host.textContent).toContain('managed-endpoint policy: Paris')
  expect(button('Open project workspace').disabled).toBe(true)
  button('S3 region').click()
  await nextTick()
  expect(document.querySelector('[role="listbox"]')?.textContent).not.toContain('Paris')
  button('S3 region').click()
  await select('S3 region', 'Rome · internal · https://rome-internal.example.org')
  expect(button('Open project workspace').disabled).toBe(true)
  await select('EC2 credential', 'key-two')
  expect(button('Open project workspace').disabled).toBe(false)
}

it('requires an explicit project, region and key when choices are ambiguous', async () => {
  await mount()
  await signInWithToken()
  expect(backend.desktop).toHaveBeenCalledWith('authenticate_openstack', {
    id: 'cloud', authentication: { method: 'token', token: 'primary' },
  })
  expect(backend.desktop.mock.calls.some(([command]) => command === 'discover_openstack_project')).toBe(false)
  await selectProjectRegionAndKey()
  button('Open project workspace').click()
  await expect.poll(() => browse.mock.calls).toEqual([[project]])
  expect(backend.desktop).toHaveBeenCalledWith('activate_openstack_project', {
    id: 'cloud', selection: { discoveryId: 'discovery', targetId: 'rome-internal', accessKey: 'key-two' },
  })
  const activated = connections.profiles.find(profile => profile.id === project.id)
  expect(workspaceAvailable(activated, 'browser')).toBe(true)
  expect(workspaceAvailable(activated, 'manager')).toBe(true)
  connections.select(project.id)
  expect(window.localStorage.getItem('storeforge.connection')).toBe(direct.id)
})

it('prompts for a fresh sign-in if the catalog changes during activation', async () => {
  activationFailure = { kind: 'reauthenticate', message: 'The OpenStack catalog target changed. Reconnect and select the region again.' }
  await mount()
  await signInWithToken()
  await selectProjectRegionAndKey()
  button('Open project workspace').click()
  await expect.poll(() => host.textContent).toContain('Sign in again')
  expect(host.textContent).toContain('The OpenStack catalog target changed')
  expect(document.querySelector<HTMLInputElement>('input[type="password"]')?.value).toBe('')
  expect(browse).not.toHaveBeenCalled()
  expect(connections.profiles).toEqual([direct])
})

it('disconnects and forgets the cloud while retaining a direct profile', async () => {
  status = [{ cloud, session, reconnectRequired: false }]
  listedProfiles = [direct, project]
  await mount()
  await expect.poll(() => host.textContent).toContain('Open workspace')
  button('Disconnect').click()
  await expect.poll(() => host.textContent).toContain('Sign in to Keystone')
  expect(backend.desktop).toHaveBeenCalledWith('disconnect_openstack_cloud', { id: 'cloud', forget: false })
  expect(connections.profiles).toEqual([direct])

  button('Forget cloud').click()
  await nextTick()
  expect(document.body.textContent).toContain('Your EC2 keys will stay in Keystone')
  document.querySelector<HTMLButtonElement>('dialog button.danger')?.click()
  await expect.poll(() => host.textContent).not.toContain('Sign in to Keystone')
  expect(backend.desktop).toHaveBeenCalledWith('disconnect_openstack_cloud', { id: 'cloud', forget: true })
  expect(connections.profiles).toEqual([direct])
})
