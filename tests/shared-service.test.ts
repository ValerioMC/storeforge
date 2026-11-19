// @vitest-environment jsdom
import { createApp, nextTick } from 'vue'
import { expect, it, vi } from 'vitest'
import SharedServiceConnections from '../src/features/connection/components/SharedServiceConnections.vue'
import { sharedServiceApi } from '../src/features/connection/api/shared-service'
import { workspaceAvailable } from '../src/features/connection/model/workspace'

const backend = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({
  desktop: backend.desktop,
  errorMessage: (error: Error) => error.message,
}))

it('keeps direct storage available while a shared service reconnects', async () => {
  const profile = { id: 'service', name: 'Research service', origin: 'https://storage.example.org' }
  const session = {
    principal: { id: 7, email: 'member@example.org', role: 'ui_admin', active: true },
    access: { manager: { available: true, contextCount: 1 }, browser: { available: true, contextCount: 1 } },
    expiresAt: '2099-07-15T12:00:00Z',
  }
  let currentSession: typeof session | null = null
  let profiles: (typeof profile)[] = []
  let contextsDenied = false
  backend.desktop.mockImplementation(async (command: string) => {
    switch (command) {
      case 'list_shared_services': return profiles.map(profile => ({ profile, session: currentSession }))
      case 'save_shared_service': profiles = [profile]; return profile
      case 'connect_shared_service': currentSession = session; return session
      case 'read_shared_contexts':
        if (contextsDenied) throw { kind: 'authorizationDenied', message: 'The service denied this request.' }
        return [{ id: 'team', kind: 'connection', name: 'Research team', connectionScope: 'shared', endpointName: 'Provider', endpointUrl: 'https://s3.example.org' }]
      case 'open_shared_action': return
      case 'disconnect_shared_service': currentSession = null; return
      default: throw new Error(command)
    }
  })
  const host = document.createElement('div')
  document.body.append(host)
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true, value: function (this: HTMLDialogElement) { this.open = true },
  })
  const app = createApp(SharedServiceConnections)
  app.mount(host)
  try {
    await settle()
    button(host, 'Add shared service').click()
    await settle()
    fill(document.body, 'Service name', profile.name)
    fill(document.body, 'Service origin', profile.origin)
    button(document.body, 'Save service').click()
    await settle()
    fill(host, 'Scoped API token', 'step-test-token')
    await nextTick()
    button(host, 'Authorize API').click()
    await settle()
    expect(host.textContent).toContain('member@example.org')
    button(host, 'Manager contexts').click()
    await settle()
    expect(host.textContent).toContain('Research team')
    contextsDenied = true
    button(host, 'Browser contexts').click()
    await settle()
    expect(host.textContent).toContain('The service denied this request.')
    expect(host.textContent).not.toContain('Research team')
    button(host, 'Account security').click()
    expect(backend.desktop).toHaveBeenCalledWith('open_shared_action', { id: profile.id, action: 'accountSecurity' })
    await settle()
    await sharedServiceApi.disconnect(profile.id, false)
    button(host, 'Refresh services').click()
    await settle()
    expect(host.textContent).not.toContain('member@example.org')
    expect(workspaceAvailable({ id: 'direct', name: 'Direct S3', provider: 'compatible', endpoint: 'https://s3.example.org', region: 'us-east-1', pathStyle: true, knownBuckets: [] }, 'browser')).toBe(true)
  } finally {
    app.unmount()
    host.remove()
  }
})

function button(host: HTMLElement, name: string): HTMLButtonElement {
  const result = [...host.querySelectorAll('button')].find(element => element.textContent?.trim() === name || element.getAttribute('aria-label') === name)
  if (!result) throw new Error(`Missing button ${name}`)
  return result
}

function fill(host: HTMLElement, label: string, value: string): void {
  const input = [...host.querySelectorAll('label')].find(element => element.textContent?.trim() === label)?.querySelector('input')
  if (!input) throw new Error(`Missing input ${label}`)
  input.value = value
  input.dispatchEvent(new Event('input'))
}

async function settle(): Promise<void> {
  await Promise.resolve()
  await nextTick()
  await new Promise(resolve => setTimeout(resolve, 0))
  await nextTick()
}
