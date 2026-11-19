// @vitest-environment jsdom
import { createApp, nextTick } from 'vue'
import { expect, it, vi } from 'vitest'
import PortalHistory from '../src/features/portal/components/PortalHistory.vue'
import PortalRequests from '../src/features/portal/components/PortalRequests.vue'
import { governanceApi } from '../src/features/portal/api/governance'
import type { PortalRequest } from '../src/features/portal/model/governance'

const backend = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({
  desktop: backend.desktop,
  errorMessage: (error: Error) => error.message,
}))

const scope = { id: 'requester', project: 12 }
const request: PortalRequest = {
  id: 7, account_id: 12, account_name: 'Research', request_type: 'account_quota_change',
  status: 'pending', payload: { target_quota_value: 50, target_quota_unit: 'GiB', direction: 'increase' },
  error_message: null, requester_email: 'requester@example.test', decided_by_email: null,
  decided_at: null, created_at: '2026-11-19T00:00:00Z', updated_at: '2026-11-19T00:00:00Z', messages: [],
}

it('shows the authoritative request decision on two clients and disables a second approval', async () => {
  let saved: PortalRequest | undefined
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true, value: function (this: HTMLDialogElement) { this.open = true },
  })
  backend.desktop.mockImplementation(async (command: string) => {
    if (command === 'read_portal') return saved ? [saved] : []
    if (command === 'change_portal') {
      saved = request
      return saved
    }
    if (command === 'read_portal_admin_requests') return { items: saved ? [saved] : [], page: 1, has_next: false }
    if (command === 'respond_portal_request') {
      if (saved?.status !== 'pending') throw new Error('Request already decided')
      saved = { ...saved, status: 'approved', decided_by_email: 'reviewer@example.test',
        messages: [{ id: 1, message: 'Approved by the service', author_email: 'reviewer@example.test', created_at: request.updated_at }] }
      return saved
    }
    throw new Error(command)
  })
  await governanceApi.requestQuota(scope, 'increase', 50, 'GiB', null)
  const requesterHost = document.createElement('div')
  const adminHost = document.createElement('div')
  document.body.append(requesterHost, adminHost)
  const requester = createApp(PortalRequests, { id: scope.id, scope, manager: true })
  const admin = createApp(PortalRequests, { id: 'admin' })
  requester.mount(requesterHost)
  admin.mount(adminHost)
  try {
    await settle()
    expect(requesterHost.textContent).toContain('pending')
    button(adminHost, '#7 Storage quota').click()
    await settle()
    button(document.body, 'Approve').click()
    await settle()
    button(document.body, 'Confirm approval').click()
    await settle()
    expect(document.body.textContent).toContain('Approved by the service')
    expect([...document.body.querySelectorAll('button')].some(button => button.textContent?.trim() === 'Approve')).toBe(false)
    button(requesterHost, 'Refresh').click()
    await settle()
    expect(requesterHost.textContent).toContain('approved')
    expect(saved?.messages).toHaveLength(1)
    expect(backend.desktop.mock.calls.filter(([command]) => command === 'respond_portal_request')).toHaveLength(1)
  } finally {
    requester.unmount()
    admin.unmount()
    requesterHost.remove()
    adminHost.remove()
  }
})

it('exposes provider history and raw export only to an eligible manager', async () => {
  backend.desktop.mockClear()
  backend.desktop.mockImplementation(async (command: string, args: { operation: { kind: string; view?: { kind: string } } }) => {
    if (command !== 'read_portal') throw new Error(command)
    if (args.operation.kind === 'state') return { server_access_logging_enabled: true }
    if (args.operation.kind === 'spaces') return []
    if (args.operation.view?.kind === 'history') return []
    if (args.operation.view?.kind === 'accessLogs') return { entries: [], offset: 0, limit: 100, total: 0 }
    throw new Error(args.operation.kind)
  })
  const managerHost = document.createElement('div')
  const memberHost = document.createElement('div')
  document.body.append(managerHost, memberHost)
  const manager = createApp(PortalHistory, { scope, manager: true })
  const member = createApp(PortalHistory, { scope, manager: false })
  manager.mount(managerHost)
  member.mount(memberHost)
  try {
    await settle()
    expect(memberHost.textContent).not.toContain('Provider access logs')
    button(managerHost, 'Provider access logs').click()
    await settle()
    expect(managerHost.textContent).toContain('Export raw logs')
    expect(backend.desktop.mock.calls.filter(([, args]) => args.operation.view?.kind === 'accessLogs')).toHaveLength(1)
  } finally {
    manager.unmount()
    member.unmount()
    managerHost.remove()
    memberHost.remove()
  }
})

function button(host: HTMLElement, label: string): HTMLButtonElement {
  const found = [...host.querySelectorAll('button')].find(button => button.textContent?.trim() === label)
  if (!found) throw new Error(`Missing button ${label}`)
  return found
}

async function settle(): Promise<void> {
  await nextTick()
  await new Promise(resolve => setTimeout(resolve, 0))
}
