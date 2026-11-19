// @vitest-environment jsdom
import { createApp, defineComponent, h, nextTick, ref } from 'vue'
import { expect, it, vi } from 'vitest'
import PortalWorkspace from '../src/features/portal/components/PortalWorkspace.vue'
import { readPortalLocation, writePortalLocation, type PortalLocation } from '../src/features/portal/model/navigation'
import { provisionSpace, spaceAccess, type PortalSpace } from '../src/features/portal/model/spaces'

const backend = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({
  desktop: backend.desktop,
  errorMessage: (error: Error) => error.message,
}))

const shared = (role: PortalSpace['role'], status: PortalSpace['status'] = 'Active'): PortalSpace => ({
  id: 'docs', name: 'Shared records', role, status, can_browse: true, can_delete: false,
  can_take_ownership: false, description: null, owner_label: 'Dana', collaborator_count: 3,
  visibility: 'shared', share_scope: 'restricted', account_member_role: null, name_editable: true,
  created_at: null, used_bytes: 120, object_count: 1, quota_max_size_bytes: 150,
  icon: { source: 'preset', preset: 'folder' },
})

it('reuses the service role across file and project workflows', () => {
  expect(spaceAccess(shared('Owner'))).toMatchObject({ managed: true, browse: true, write: true })
  expect(spaceAccess(shared('Viewer'))).toMatchObject({ managed: false, browse: true, write: false })
  expect(spaceAccess(shared('Editor'))).toMatchObject({ managed: false, browse: true, write: true })
  expect(spaceAccess(shared('Manager', 'Archived'))).toMatchObject({ managed: true, write: false })
  expect(provisionSpace('selected', ' Records ', 'Editor', [{ user_id: 22, role: 'Viewer' }])).toEqual({
    description: 'Records', visibility: 'shared', share_scope: 'restricted', account_member_role: null,
    initial_shares: [{ user_id: 22, role: 'Viewer' }],
  })
})

it('round-trips the service, project, folder, and exact object key in the query', () => {
  const location = {
    page: 'spaces', serviceId: 'service', projectId: 12, spaceId: 'docs', folder: 'shared +%/',
    object: 'shared +%/a#b?.txt', fileView: 'versions', view: 'files',
  } as const
  const url = new URL('https://desktop.local/?keep=1')
  writePortalLocation(url, location)
  expect(readPortalLocation(url.search)).toEqual(location)
  expect(readPortalLocation('?portal=spaces&project=12&spaceId=docs')).toBeUndefined()
  writePortalLocation(url, undefined)
  expect(url.search).toBe('?keep=1')
})

it('opens a shared file by scope and removes write actions when the space is archived', async () => {
  const location = ref<PortalLocation>({ page: 'spaces', serviceId: 'service', projectId: 12, spaceId: 'docs', folder: 'shared/' })
  let archived = false
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value: function (this: HTMLDialogElement) { this.open = true } })
  backend.desktop.mockImplementation(async (command: string, args: { project?: number; operation?: { kind: string; key?: string } }) => {
    if (command === 'list_shared_services') return [{
      profile: { id: 'service', name: 'Research', origin: 'https://service.example.org' },
      session: { principal: { id: 17 }, access: { portal: { available: true } } },
    }]
    if (command === 'change_portal') {
      archived = !archived
      return shared('Owner', archived ? 'Archived' : 'Active')
    }
    switch (args.operation?.kind) {
      case 'projects': return [{ id: 12, name: 'Research', portal_role: 'portal_manager' }, { id: 34, name: 'Other', portal_role: 'portal_user' }]
      case 'spaces': return [shared(args.project === 12 ? 'Owner' : 'Viewer')]
      case 'state': return {}
      case 'space': return shared(args.project === 12 ? 'Owner' : 'Viewer', archived ? 'Archived' : 'Active')
      case 'files': return { objects: [{ key: 'shared/note.txt', size: 16, last_modified: null }], prefixes: [], next_continuation_token: null }
      case 'detail': return { key: args.operation.key, name: 'note.txt', preview_type: 'text', preview_text: 'Project file' }
      default: throw new Error(command)
    }
  })
  const host = document.createElement('div')
  document.body.append(host)
  const app = createApp(defineComponent({
    setup: () => () => h(PortalWorkspace, { location: location.value, onNavigate: value => { location.value = value } }),
  }))
  app.mount(host)
  try {
    await settle()
    button(host, 'note.txt').click()
    await settle()
    expect(document.body.textContent).toContain('Project file')
    expect(location.value).toMatchObject({ projectId: 12, folder: 'shared/', object: 'shared/note.txt' })
    button(document.body, 'Close dialog').click()
    await settle()
    button(host, 'Archive space').click()
    await settle()
    button(document.body, 'Confirm').click()
    await settle()
    expect(host.textContent).toContain('This space is archived')
    expect(host.textContent).not.toContain('New folder')
    button(host, 'Restore space').click()
    await settle()
    button(document.body, 'Confirm').click()
    await settle()
    expect(host.textContent).toContain('New folder')
    location.value = { page: 'spaces', serviceId: 'service', projectId: 34, spaceId: 'docs' }
    await settle()
    expect(host.textContent).toContain('Viewer')
    expect(host.textContent).not.toContain('New folder')
    expect(backend.desktop).toHaveBeenCalledWith('read_portal', {
      id: 'service', project: 34, space: 'docs', operation: { kind: 'files', prefix: '', cursor: undefined },
    })
  } finally {
    app.unmount()
    host.remove()
    vi.restoreAllMocks()
  }
})

async function settle(): Promise<void> {
  await nextTick()
  await new Promise(resolve => setTimeout(resolve, 80))
  await nextTick()
}

function button(host: HTMLElement, name: string): HTMLButtonElement {
  const found = [...host.querySelectorAll('button')]
    .find(button => button.textContent?.trim() === name || button.getAttribute('aria-label') === name)
  if (!found) throw new Error(`Missing button ${name}`)
  return found
}
