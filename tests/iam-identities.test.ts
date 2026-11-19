// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, type Component } from 'vue'
import { createPinia } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { createIdentity } from '../src/features/administration/api/iam'
import IamIdentities from '../src/features/administration/components/IamIdentities.vue'
import IamKeys from '../src/features/administration/components/IamKeys.vue'
import IamPicker from '../src/features/administration/components/IamPicker.vue'
import IamPrivateAccess from '../src/features/administration/components/IamPrivateAccess.vue'
import { operations, type AdminRequest } from '../src/features/administration/model/operations'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }))
const desktop = vi.mocked(invoke)
const mounts: (() => void)[] = []
const storageWrites = vi.fn()

beforeEach(() => {
  vi.stubGlobal('localStorage', { getItem: vi.fn(), setItem: storageWrites, removeItem: vi.fn() })
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true,
    value(this: HTMLDialogElement) { this.open = true },
  })
})

afterEach(() => {
  mounts.splice(0).reverse().forEach(cleanup => cleanup())
  desktop.mockReset()
  storageWrites.mockReset()
  vi.unstubAllGlobals()
})

function mount(component: Component, props: Record<string, unknown>): ParentNode {
  const host = document.body.appendChild(document.createElement('div'))
  const app = createApp({ render: () => h(component, props) })
  app.use(createPinia())
  app.mount(host)
  mounts.push(() => { app.unmount(); host.remove() })
  return host
}

function button(root: ParentNode, label: string): HTMLButtonElement {
  const found = Array.from(root.querySelectorAll('button')).find(item => item.textContent?.trim() === label)
  if (!found) throw new Error(`Button ${label} is unavailable`)
  return found
}

function change(input: HTMLInputElement, value: string): void {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function query(action: string, body = ''): string {
  return `<${action}Response xmlns="urn:iam"><${action}Result>${body}</${action}Result></${action}Response>`
}

function user(name: string): string {
  return `<UserId>fixed-user-id</UserId><UserName>${name}</UserName><Arn>arn:fixture:user/${name}</Arn><Path>/teams/</Path>`
}

function iamMock(handler: (action: string, values: AdminRequest['values']) => string): void {
  desktop.mockImplementation(async (command, arguments_) => {
    if (command !== 'administer') throw new Error(`Unexpected command ${command}`)
    const request = (arguments_ as { request: AdminRequest }).request
    const operation = operations.find(item => item.id === request.operation)
    if (!operation) throw new Error(`Unknown operation ${request.operation}`)
    expect(request.confirmed).toBe(operation.write)
    return handler(operation.action, request.values)
  })
}

describe('IAM user, group and personal access', () => {
  it('reports a failed membership attachment and reloads the created identity', async () => {
    const calls: string[] = []
    iamMock((action, values) => {
      calls.push(action)
      if (action === 'CreateUser' || action === 'GetUser') return query(action, `<User>${user(values.UserName!)}</User>`)
      if (action === 'AddUserToGroup') throw new Error('Membership denied')
      if (action === 'AttachUserPolicy') return query(action)
      throw new Error(action)
    })
    const result = await createIdentity('fixture', 'User', { name: 'new-user', path: '/teams/', members: ['audit'], policies: ['arn:policy/read-only'] })
    expect(result.identity.name).toBe('new-user')
    expect(result.failures).toEqual(['Membership for audit: Membership denied'])
    expect(calls).toEqual(['CreateUser', 'AddUserToGroup', 'AttachUserPolicy', 'GetUser'])
  })

  it('opens the second candidate page using the provider marker', async () => {
    iamMock((action, values) => {
      expect(action).toBe('ListGroups')
      const name = values.Marker ? 'second-group' : 'first-group'
      return query(action, `<Groups><member><GroupId>${name}-id</GroupId><GroupName>${name}</GroupName></member></Groups>${values.Marker ? '' : '<IsTruncated>true</IsTruncated><Marker>next&amp;opaque</Marker>'}`)
    })
    const host = mount(IamPicker, { connection: 'fixture', kind: 'Group', label: 'Groups', modelValue: [] })
    await vi.waitFor(() => button(host, 'Next page').click())
    await vi.waitFor(() => expect(button(host, 'Previous page').disabled).toBe(false))
    expect(desktop).toHaveBeenCalledWith('administer', {
      connection: 'fixture', request: expect.objectContaining({ values: { Marker: 'next&opaque' } }),
    })
  })

  it('renames an identity and preserves the selected access-keys tab', async () => {
    let name = 'original'
    iamMock((action, values) => {
      if (action === 'ListUsers') return query(action, `<Users><member>${user(name)}</member></Users>`)
      if (action === 'GetUser') return query(action, `<User>${user(values.UserName!)}</User>`)
      if (action === 'ListAccessKeys') return query(action, '<AccessKeyMetadata/>')
      if (action === 'UpdateUser') {
        expect(values).toMatchObject({ UserName: 'original', NewUserName: 'renamed' })
        name = values.NewUserName!
        return query(action)
      }
      throw new Error(action)
    })
    const host = mount(IamIdentities, { connection: 'fixture', kind: 'User' })
    await vi.waitFor(() => button(host, 'Open').click())
    await vi.waitFor(() => button(host, 'Access keys').click())
    await vi.waitFor(() => expect(host.textContent).toContain('No access keys.'))
    button(host, 'Edit user').click()
    await vi.waitFor(() => {
      const input = document.querySelector<HTMLInputElement>('dialog input')
      expect(input).toBeTruthy()
      change(input!, 'renamed')
    })
    await vi.waitFor(() => expect(button(document, 'Save changes').disabled).toBe(false))
    document.querySelector('dialog form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await vi.waitFor(() => {
      expect(host.querySelector('h2')?.textContent).toBe('renamed')
      expect(button(host, 'Access keys').getAttribute('aria-pressed')).toBe('true')
      expect(desktop).toHaveBeenCalledWith('administer', { connection: 'fixture', request: expect.objectContaining({ operation: 'access-keys-list-access-keys', values: { UserName: 'renamed' } }) })
    })
  })

  it('shows an issued key once and its remote owner without persisting the secret', async () => {
    iamMock((action) => {
      if (action === 'ListAccessKeys') return query(action, '<AccessKeyMetadata/>')
      return query(action, '<AccessKey><UserName>owner</UserName><AccessKeyId>key-id</AccessKeyId><Status>Active</Status><SecretAccessKey>one-time-fixture</SecretAccessKey></AccessKey>')
    })
    const host = mount(IamKeys, { connection: 'fixture', user: { id: 'u1', name: 'owner' }, readOnly: false })
    button(host, 'Create access key').click()
    await vi.waitFor(() => expect(document.querySelector('dialog form')).toBeTruthy())
    document.querySelector('dialog form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await vi.waitFor(() => expect(Array.from(document.querySelectorAll<HTMLInputElement>('dialog input'), item => item.value)).toEqual(['owner', 'key-id', 'one-time-fixture']))
    button(document, 'Done').click()
    await vi.waitFor(() => expect(document.querySelector('dialog')).toBeNull())
    expect(storageWrites).not.toHaveBeenCalled()
  })

  it('reopens a linked private profile using its ID and metadata alone', async () => {
    const profile = { id: 'private-id', name: 'Daily storage', provider: 'ceph', region: 'zone-a', knownBuckets: [] }
    desktop.mockImplementation(async command => {
      if (command === 'list_private_iam') return [{ id: profile.id, name: profile.name, status: 'active', userName: 'storeforge-private-fixture' }]
      if (command === 'list_connections') return [profile]
      throw new Error(command)
    })
    const open = vi.fn()
    const host = mount(IamPrivateAccess, { profile: { id: 'administrator' }, onOpen: open })
    await vi.waitFor(() => button(host, 'Open connection').click())
    await vi.waitFor(() => expect(open).toHaveBeenCalledWith(profile))
    expect(desktop.mock.calls.map(([command]) => command)).toEqual(['list_private_iam', 'list_connections'])
  })
})
