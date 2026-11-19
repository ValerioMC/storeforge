// @vitest-environment jsdom
import { createApp, nextTick, type Component } from 'vue'
import { beforeAll, expect, it, vi } from 'vitest'
import WorkspacePortalPolicies from '../src/features/administration/components/WorkspacePortalPolicies.vue'
import WorkspaceRotation from '../src/features/administration/components/WorkspaceRotation.vue'
import type { RotationEndpoint, RotationReport } from '../src/features/administration/model/workspace-settings'

const backend = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({
  desktop: backend.desktop,
  errorMessage: (error: Error) => error.message,
}))

beforeAll(() => {
  if (!HTMLDialogElement.prototype.showModal) {
    HTMLDialogElement.prototype.showModal = function (): void { this.open = true }
  }
})

it('keeps a saved account override visible after the service prohibits it', async () => {
  let stored: boolean | null = null
  let platformEnabled = true
  backend.desktop.mockImplementation(async (command: string, args: { change?: { value: boolean | null } }) => {
    if (command === 'workspace_policy_accounts') return { items: [{ id: 23, name: 'Research' }], has_next: false }
    if (command !== 'workspace_portal_account') throw new Error(command)
    if (args.change) stored = args.change.value
    return { admin_override: { browser_access_enabled: stored }, effective: { browser_access_enabled: platformEnabled && stored } }
  })
  const { host, cleanup } = mount(WorkspacePortalPolicies)
  try {
    await settle()
    await select(host, 'Portal account', 'Research')
    await select(host, 'Browser access override', 'Allow')
    expect(stored).toBe(true)
    expect(host.textContent).toContain('Effective: Enabled')
    platformEnabled = false
    await select(host, 'Portal account', 'Research')
    expect(host.textContent).toContain('Effective: Disabled')
    expect(host.querySelector('[aria-label="Browser access override"]')?.textContent).toContain('Allow')
  } finally {
    cleanup()
  }
})

it('resumes a prepared service rotation after a lost reply and displays individual results', async () => {
  let phase: 'prepared' | null = null
  const endpoint = (): RotationEndpoint => ({
    id: 12, name: 'East', provider: 'ceph', is_editable: true, capabilities: { admin: true },
    service_identities: [{ kind: 'runtime', mode: 'managed', status: 'ready', rotation_phase: phase }],
  })
  const report: RotationReport = {
    summary: { rotated: 1, failed: 1, skipped: 1, deleted_old_keys: 1, disabled_old_keys: 0 },
    results: [
      { endpoint_id: 12, endpoint_name: 'East', key_type: 'endpoint_runtime', target_id: '1', target_label: 'Replacement', status: 'rotated', message: 'Revalidated', rotation_phase: null },
      { endpoint_id: 12, endpoint_name: 'East', key_type: 's3_user', target_id: '2', target_label: 'Writer', status: 'failed', message: 'Permissions changed', rotation_phase: 'activated' },
      { endpoint_id: 12, endpoint_name: 'East', key_type: 'account', target_id: '3', target_label: 'Archive', status: 'skipped', message: 'Externally managed', rotation_phase: null },
    ],
  }
  backend.desktop.mockImplementation(async (command: string) => {
    switch (command) {
      case 'workspace_rotation_endpoints': return [endpoint()]
      case 'rotate_workspace_keys':
        phase = 'prepared'
        throw new Error('Reply lost')
      case 'recover_workspace_rotation':
        phase = null
        return report
      default: throw new Error(command)
    }
  })
  const { host, cleanup } = mount(WorkspaceRotation)
  try {
    await settle()
    checkbox(host, 'Runtime').click()
    await settle()
    checkbox(host, 'East').click()
    await settle()
    button(host, 'Review rotation').click()
    await settle()
    button(document, 'Start rotation').click()
    await settle()
    expect(host.textContent).toContain('Reply lost')
    expect(host.textContent).toContain('Runtime · Prepared')
    expect(button(host, 'Review rotation').disabled).toBe(true)
    button(host, 'Resume Runtime').click()
    await settle()
    button(document, 'Resume rotation').click()
    await settle()
    expect(backend.desktop).toHaveBeenCalledWith('recover_workspace_rotation', {
      id: 'research', recovery: { endpoint_id: 12, kind: 'endpoint_runtime', phase: 'prepared', deactivate_only: false },
    })
    expect(host.textContent).toContain('1 rotated · 1 failed · 1 skipped')
    for (const text of ['Replacement', 'Revalidated', 'Writer', 'Permissions changed', 'Archive', 'Externally managed']) {
      expect(host.textContent).toContain(text)
    }
  } finally {
    cleanup()
  }
})

function mount(component: Component) {
  const host = document.createElement('div')
  document.body.append(host)
  const app = createApp(component, { serviceId: 'research' })
  app.mount(host)
  return { host, cleanup: () => { app.unmount(); host.remove() } }
}

function button(host: ParentNode, label: string): HTMLButtonElement {
  const result = [...host.querySelectorAll('button')].find(element => element.textContent?.trim() === label)
  if (!result) throw new Error(`Missing button ${label}`)
  return result
}

function checkbox(host: ParentNode, label: string): HTMLInputElement {
  const result = [...host.querySelectorAll('label')].find(element => element.textContent?.trim() === label)?.querySelector('input')
  if (!result) throw new Error(`Missing checkbox ${label}`)
  return result
}

async function select(host: HTMLElement, label: string, value: string): Promise<void> {
  host.querySelector<HTMLButtonElement>(`[role="combobox"][aria-label="${label}"]`)?.click()
  await settle()
  const option = [...document.querySelectorAll<HTMLButtonElement>('[role="option"]')].find(element => element.textContent?.trim() === value)
  if (!option) throw new Error(`Missing option ${value}`)
  option.click()
  await settle()
}

async function settle(): Promise<void> {
  await nextTick()
  await new Promise(resolve => setTimeout(resolve, 0))
  await nextTick()
}
