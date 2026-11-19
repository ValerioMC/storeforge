// @vitest-environment jsdom
import { createApp, nextTick } from 'vue'
import { expect, it, vi } from 'vitest'
import PlatformGovernance from '../src/features/administration/components/PlatformGovernance.vue'
import type { AccessReviewRow } from '../src/features/administration/model/platform-governance'

const backend = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({
  desktop: backend.desktop,
  errorMessage: (error: Error) => error.message,
}))

it('refreshes effective grants, preserves inactive status, and opens the service review action', async () => {
  let denied = false
  const group = { kind: 'group', groupName: 'Operators', groupId: 31 } as const
  let sources: AccessReviewRow['rights'][number]['sources'] = [{ kind: 'direct', groupName: null, groupId: null }, group]
  backend.desktop.mockImplementation(async (command: string) => {
    switch (command) {
      case 'list_shared_services': return [{
        profile: { id: 'research', name: 'Research service', origin: 'https://storage.example.org' },
        session: { principal: { email: 'admin@example.org', role: 'ui_admin' }, access: { admin: { available: true } }, expiresAt: '2099-07-15T12:00:00Z' },
      }]
      case 'list_platform_access':
        if (denied) throw { kind: 'authorizationDenied', message: 'The service denied this request.' }
        return { page: 1, hasNext: false, total: 1, items: [{
          key: 'principal:7:connection:10',
          principal: { id: 7, fullName: 'Researcher', email: 'reader@example.org', role: 'ui_user', isActive: false },
          target: { id: 10, name: 'Research archive', identifier: null },
          scope: 's3_connection',
          rights: [{ code: 'shared_connection_access', sources }],
        }] }
      case 'export_platform_access': return true
      case 'open_shared_action': return
      default: throw new Error(command)
    }
  })
  const host = document.createElement('div')
  document.body.append(host)
  const app = createApp(PlatformGovernance)
  app.mount(host)
  try {
    await settle()
    expect(host.textContent).toContain('Research archive')
    expect(host.textContent).toContain('Inactive')
    expect(host.textContent).toContain('Direct')
    expect(host.textContent).toContain('Operators')
    button(host, 'Review user').click()
    await settle()
    expect(backend.desktop).toHaveBeenCalledWith('open_shared_action', {
      id: 'research', action: { reviewUser: { id: 7, search: 'reader@example.org' } },
    })
    sources = [group]
    button(host, 'Refresh').click()
    await settle()
    expect(host.textContent).toContain('Operators')
    expect(host.textContent).not.toContain('Direct')
    button(host, 'Export CSV').click()
    await settle()
    expect(backend.desktop).toHaveBeenCalledWith('export_platform_access', {
      id: 'research', filters: expect.not.objectContaining({ page: expect.anything() }),
    })
    denied = true
    button(host, 'Refresh').click()
    await settle()
    expect(host.textContent).toContain('The service denied this request.')
    expect(host.textContent).not.toContain('Research archive')
  } finally {
    app.unmount()
    host.remove()
  }
})

function button(host: HTMLElement, name: string): HTMLButtonElement {
  const result = [...host.querySelectorAll('button')].find(element => element.textContent?.trim() === name)
  if (!result) throw new Error(`Missing button ${name}`)
  return result
}

async function settle(): Promise<void> {
  await nextTick()
  await new Promise(resolve => setTimeout(resolve, 0))
  await nextTick()
}
