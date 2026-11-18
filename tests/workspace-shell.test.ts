// @vitest-environment jsdom

import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, expect, it, vi } from 'vitest'
import { desktop } from '../src/common/tauri'
import { useBrowser } from '../src/features/browser/store/browser'
import { endpointSettings, type Connection } from '../src/features/connection/model/connection'
import { discardSetup, pausedSetup, pauseSetup } from '../src/features/connection/model/setup-draft'
import { defaultRoute, workspaceAvailable } from '../src/features/connection/model/workspace'
import { useWorkspaceNavigation } from '../src/features/connection/store/workspace-navigation'

vi.mock('../src/common/tauri', () => ({
  desktop: vi.fn(),
  isTauri: () => true,
  errorMessage: String,
}))

function profile(id: string): Connection {
  return { id, name: id, provider: 'ceph', endpoint: 'https://storage.example.test',
    region: 'us-east-1', pathStyle: true, knownBuckets: ['archive'], settings: endpointSettings() }
}

beforeEach(() => {
  const values = new Map<string, string>()
  Object.defineProperty(window, 'localStorage', { configurable: true, value: {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value) },
    removeItem: (key: string) => { values.delete(key) },
  } })
  vi.mocked(desktop).mockImplementation(command => Promise.resolve(command === 'list_buckets' ? [] : { rows: [], cursor: null }))
  setActivePinia(createPinia())
})

it('moves backward and forward across both resource paths and connections', () => {
  const navigation = useWorkspaceNavigation()
  navigation.initialize(defaultRoute(profile('tenant-a')))
  navigation.go({ workspace: 'browser', connectionId: 'tenant-a', bucket: 'archive', prefix: 'logs/' })
  navigation.go({ workspace: 'manager', connectionId: 'tenant-b', page: 'buckets', bucket: 'archive' })

  navigation.back()
  expect(navigation.current).toMatchObject({ connectionId: 'tenant-a', bucket: 'archive', prefix: 'logs/' })
  navigation.forward()
  expect(navigation.current).toMatchObject({ connectionId: 'tenant-b', workspace: 'manager', bucket: 'archive' })

  setActivePinia(createPinia())
  const reopened = useWorkspaceNavigation()
  reopened.initialize(defaultRoute(profile('tenant-a')))
  expect(reopened.current).toEqual(navigation.current)
  expect(reopened.canBack).toBe(true)
})

it('preserves Browser view state at return and isolates the same path in another profile', async () => {
  const browser = useBrowser()
  await browser.changeConnection('tenant-a')
  await browser.navigate('archive', 'logs/')
  browser.filter = 'tomorrow'
  browser.toggleSelection('logs/one.txt')
  await browser.changeConnection('tenant-b')
  await browser.navigate('archive', 'logs/')
  expect(browser.filter).toBe('')
  expect(browser.selected).toEqual([])
  await browser.changeConnection('tenant-a')
  await browser.navigate('archive', 'logs/')
  expect(browser.filter).toBe('tomorrow')
  expect(browser.selected).toEqual(['logs/one.txt'])
})

it('honors independent Browser and Manager access, including the default workspace', () => {
  const connection = profile('tenant-a')
  connection.settings = { ...endpointSettings(), browserEnabled: false, defaultWorkspace: 'browser' }
  expect(workspaceAvailable(connection, 'browser')).toBe(false)
  expect(workspaceAvailable(connection, 'manager')).toBe(true)
  expect(defaultRoute(connection)).toMatchObject({ connectionId: 'tenant-a', workspace: 'manager' })
})

it('resumes paused setup at credential checks without persisting secrets', () => {
  const connection = profile('draft')
  const draft = {
    ...connection,
    settings: { ...endpointSettings(), browserEnabled: false, defaultWorkspace: 'manager' as const },
    credentials: { accessKey: 'fixture-access', secretKey: 'fixture-secret', sessionToken: null },
  }
  pauseSetup(draft, 'review')
  const raw = window.localStorage.getItem('storeforge.connection-setup')
  expect(raw).not.toContain('fixture-access')
  expect(raw).not.toContain('fixture-secret')
  expect(pausedSetup()).toMatchObject({
    stage: 'credentials', profile: { name: 'draft', settings: { browserEnabled: false, defaultWorkspace: 'manager' } },
  })
  discardSetup()
  expect(pausedSetup()).toBeUndefined()
})
