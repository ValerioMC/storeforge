// @vitest-environment jsdom
import { createApp, nextTick, type Component } from 'vue'
import { expect, it, vi } from 'vitest'
import SharedAuditHistory from '../src/features/audit/components/SharedAuditHistory.vue'
import SharedActivity from '../src/features/audit/components/SharedActivity.vue'
import OperationalWebhooks from '../src/features/audit/components/OperationalWebhooks.vue'
import type { ServiceAuditEvent, WebhookDelivery } from '../src/features/audit/model/operational-events'

const backend = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({
  desktop: backend.desktop,
  errorMessage: (error: Error) => error.message,
}))

const firstEvent: ServiceAuditEvent = {
  id: 10, created_at: '2026-11-18T09:24:00Z', user_email: 'ops@example.org', action: 'bucket.configure',
  entity_type: 'bucket', entity_id: 'photos', context_name: 'Research', status: 'partial',
  user_role: null, scope: null, message: null, metadata: null,
}

it('loads Manager events only for the chosen context and keeps Admin events distinct', async () => {
  backend.desktop.mockImplementation(async (command: string, args: { scope?: string; filter?: { context_id?: string } }) => {
    if (command === 'read_shared_contexts') return [{ id: 'account:12', name: 'Research' }, { id: 'account:13', name: 'Archive' }]
    if (command !== 'operational_events') throw new Error(command)
    if (args.scope === 'admin') return { logs: [{ ...firstEvent, id: 31, action: 'settings.update', scope: 'admin' }], next_cursor: null }
    if (args.filter?.context_id === 'account:12') return { logs: [firstEvent], next_cursor: null }
    return { logs: [], next_cursor: null }
  })
  const manager = mount(SharedAuditHistory, { scope: 'manager' })
  const admin = mount(SharedAuditHistory, { scope: 'admin' })
  try {
    await settle()
    expect(manager.host.textContent).not.toContain('Bucket configure')
    await selectContext(manager.host, 'Research')
    expect(manager.host.textContent).toContain('Bucket configure')
    expect(manager.host.textContent).toContain('ops@example.org')
    expect(manager.host.textContent).toContain('Partial')
    expect(admin.host.textContent).toContain('Settings update')
    expect(admin.host.textContent).not.toContain('Bucket configure')
    await selectContext(manager.host, 'Archive')
    expect(manager.host.textContent).toContain('No matching events')
    expect(backend.desktop).toHaveBeenCalledWith('operational_events', {
      id: 'service-a', scope: 'manager', filter: expect.objectContaining({ context_id: 'account:13' }),
    })
  } finally {
    manager.cleanup()
    admin.cleanup()
  }
})

it('updates the unread badge from the service after marking a quota notification read', async () => {
  let read = false
  backend.desktop.mockImplementation(async (command: string) => {
    switch (command) {
      case 'list_shared_services': return [{
        profile: { id: 'service-a', name: 'Research', origin: 'https://example.org' },
        session: { principal: { role: 'ui_admin' }, access: { admin: { available: true } }, expiresAt: '2099-11-19T10:00:00Z' },
      }]
      case 'open_shared_action': return
      case 'operational_events': return { logs: [], next_cursor: null }
      case 'operational_inbox': return { unread_count: read ? 0 : 1, items: [
        { id: 2, title: 'Quota threshold reached', type: 'quota_alert', message: 'Research exceeded 85 percent.',
          severity: 'warning', created_at: '2026-11-18T09:24:00Z', read_at: read ? '2026-11-18T09:32:00Z' : null },
      ] }
      case 'operational_quota_preferences': return { quota_alerts_enabled: false, quota_alerts_global_watch: false }
      case 'read_operational_notifications':
        read = true
        return { unread_count: 0 }
      default: throw new Error(command)
    }
  })
  const view = mount(SharedActivity)
  try {
    await settle()
    button(view.host, 'Inbox (1)').click()
    await vi.waitFor(() => expect(view.host.textContent?.match(/Quota threshold reached/g) ?? []).toHaveLength(1))
    button(view.host, 'Mark read').click()
    await settle()
    expect(read).toBe(true)
    expect(button(view.host, 'Inbox')).toBeTruthy()
    expect(view.host.textContent).toContain('Read')
    button(view.host, 'Provider access').click()
    await vi.waitFor(() => expect(view.host.textContent).toContain('S3 provider access logs'))
    expect(view.host.textContent).not.toContain('Quota threshold reached')
    button(view.host, 'Open provider logs').click()
    expect(backend.desktop).toHaveBeenCalledWith('open_shared_action', { id: 'service-a', action: 'providerAccessLogs' })
  } finally {
    view.cleanup()
  }
})

it('keeps the original event on a retried delivery and shows the latest response', async () => {
  let delivered = false
  const delivery = (): WebhookDelivery => ({
    id: 54, event_type: 'admin.settings.update', is_test: false, status: delivered ? 'delivered' : 'retrying',
    attempt_count: delivered ? 2 : 1, last_http_status: delivered ? 204 : 429, last_error: delivered ? null : 'Rate limited',
    updated_at: '2026-11-18T09:24:00Z', delivered_at: delivered ? '2026-11-18T09:24:00Z' : null,
  })
  backend.desktop.mockImplementation(async (command: string) => {
    if (command === 'operational_webhooks') return [{
      id: 7, name: 'Operations', url: 'https://example.org/events', enabled: true,
      event_types: ['admin.settings.update'], has_signing_secret: true, last_delivery: delivery(),
    }]
    if (command === 'operational_webhook_deliveries') return [delivery()]
    if (command === 'open_shared_action') return
    throw new Error(command)
  })
  const view = mount(OperationalWebhooks, { authorized: true })
  try {
    await settle()
    button(view.host, 'Deliveries').click()
    await settle()
    expect(view.host.textContent).toContain('Delivery 54')
    expect(view.host.textContent).toContain('Retrying')
    expect(view.host.textContent).toContain('Last response HTTP 429')
    delivered = true
    button(view.host, 'Refresh webhooks').click()
    await settle()
    expect(view.host.textContent).toContain('Delivery 54')
    expect(view.host.textContent).toContain('admin.settings.update')
    expect(view.host.textContent).toContain('2 attempts')
    expect(view.host.textContent).toContain('Last response HTTP 204')
    expect(view.host.textContent).not.toContain('Rate limited')
    button(view.host, 'Edit in browser').click()
    await settle()
    expect(backend.desktop).toHaveBeenCalledWith('open_shared_action', { id: 'service-a', action: { editWebhook: { id: 7 } } })
  } finally {
    view.cleanup()
  }
})

function mount(component: Component, properties: Record<string, unknown> = {}) {
  const host = document.createElement('div')
  document.body.append(host)
  const app = createApp(component, { serviceId: 'service-a', ...properties })
  app.mount(host)
  return { host, cleanup: () => { app.unmount(); host.remove() } }
}

function button(host: ParentNode, label: string): HTMLButtonElement {
  const result = [...host.querySelectorAll('button')].find(element => element.textContent?.trim() === label)
  if (!result) throw new Error(`Missing button ${label}`)
  return result
}

async function selectContext(host: HTMLElement, label: string): Promise<void> {
  host.querySelector<HTMLButtonElement>('[aria-label="Manager activity context"]')?.click()
  await settle()
  const option = [...document.querySelectorAll<HTMLButtonElement>('[role="option"]')].find(element => element.textContent?.trim() === label)
  if (!option) throw new Error(`Missing context ${label}`)
  option.click()
  await settle()
}

async function settle(): Promise<void> {
  await nextTick()
  await new Promise(resolve => setTimeout(resolve, 0))
  await nextTick()
}
