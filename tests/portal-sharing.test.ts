// @vitest-environment jsdom
import { createApp, nextTick } from 'vue'
import { expect, it, vi } from 'vitest'
import PortalSpaceSharing from '../src/features/portal/components/PortalSpaceSharing.vue'
import { sharingApi } from '../src/features/portal/api/sharing'
import { clientConnection, clientDocument } from '../src/features/portal/model/client-export'

const backend = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({
  desktop: backend.desktop,
  errorMessage: (error: Error) => error.message,
}))

const project = { id: 'service', project: 12 }
const space = { ...project, space: 'docs' }

it('exports client connection fields and keeps one-time credentials out of every file', () => {
  const key = { access_key_id: 'AK&EXT', bucket_name: 'docs-data', secret_access_key: 'one-time', session_token: 'runtime-token' }
  const connection = clientConnection('https://s3.example.test:9443', key, true, 'region-1')
  expect(connection).toBeDefined()
  if (!connection) throw new Error('Missing client settings')
  for (const format of ['cyberduck', 'winscp', 'generic'] as const) {
    const document = clientDocument(format, connection)
    expect(document).toContain('docs-data')
    expect(document).toContain('s3.example.test')
    expect(document).not.toContain('one-time')
    expect(document).not.toContain('runtime-token')
  }
  expect(clientDocument('cyberduck', connection)).toContain('AK&amp;EXT')
  expect(clientDocument('winscp', connection)).toContain('S3UrlStyle=1')
})

it('passes the project, space, and exact scoped access actions to the authority', async () => {
  backend.desktop.mockResolvedValue({})
  await sharingApi.memberRole(project, 48, 'portal_manager', null)
  await sharingApi.grant(space, 48, 'Editor')
  await sharingApi.externalKey(project, space.space, 'Partner', 'read_only')
  expect(backend.desktop.mock.calls.slice(-3)).toEqual([
    ['change_portal', { ...project, operation: { kind: 'sharing', action: { kind: 'memberRole', target_user_id: 48, portal_role: 'portal_manager', reason: null } } }],
    ['change_portal', { ...space, operation: { kind: 'sharing', action: { kind: 'grant', user_id: 48, role: 'Editor' } } }],
    ['change_portal', { ...project, operation: { kind: 'sharing', action: { kind: 'externalKey', storage_space_id: 'docs', external_email: 'Partner', permission: 'read_only' } } }],
  ])
})

it('revokes a public link and refreshes the service outcome', async () => {
  let status = 'Active'
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true, value: function (this: HTMLDialogElement) { this.open = true },
  })
  backend.desktop.mockImplementation(async (command: string, args: { operation: { view?: { kind: string }; action?: { kind: string } } }) => {
    if (command === 'change_portal' && args.operation.action?.kind === 'revokeLink') {
      status = 'Revoked'
      return []
    }
    if (args.operation.view?.kind === 'access') return {
      mode: 'restricted', explicit_shares: [], effective_member_count: 2, public_link_count: 1,
      can_create_public_links: true, can_manage_access: true,
    }
    if (args.operation.view?.kind === 'links') return [{ id: 3, object_name: 'notes.txt', url: 'https://service.example.test/public', status }]
    throw new Error(command)
  })
  const host = document.createElement('div')
  document.body.append(host)
  const app = createApp(PortalSpaceSharing, { scope: space, status: 'Active' })
  app.mount(host)
  try {
    await settle()
    button(host, 'Revoke link').click()
    await settle()
    button(document.body, 'Confirm').click()
    await settle()
    expect(host.textContent).toContain('Revoked')
    expect(host.textContent).not.toContain('Copy link')
    expect(backend.desktop).toHaveBeenCalledWith('change_portal', {
      ...space, operation: { kind: 'sharing', action: { kind: 'revokeLink', link_id: 3 } },
    })
  } finally {
    app.unmount()
    host.remove()
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
