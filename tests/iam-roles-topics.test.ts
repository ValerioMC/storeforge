// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, type Component } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listTopics } from '../src/features/administration/api/resources'
import IamManagedPolicies from '../src/features/administration/components/IamManagedPolicies.vue'
import IamRoles from '../src/features/administration/components/IamRoles.vue'
import { operations, type AdminRequest } from '../src/features/administration/model/operations'
import BucketSettings from '../src/features/bucket/components/BucketSettings.vue'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }))
const desktop = vi.mocked(invoke)
const mounts: (() => void)[] = []

afterEach(() => {
  mounts.splice(0).reverse().forEach(cleanup => cleanup())
  desktop.mockReset()
})

function mount(component: Component, props: Record<string, unknown>): ParentNode {
  const host = document.body.appendChild(document.createElement('div'))
  const app = createApp({ render: () => h(component, props) })
  app.mount(host)
  mounts.push(() => { app.unmount(); host.remove() })
  return host
}

function button(root: ParentNode, label: string): HTMLButtonElement {
  const found = Array.from(root.querySelectorAll('button')).find(item => item.textContent?.trim() === label)
  if (!found) throw new Error(`Button ${label} is unavailable`)
  return found
}

function query(action: string, body = ''): string {
  return `<${action}Response xmlns="urn:provider"><${action}Result>${body}</${action}Result></${action}Response>`
}

function mockQueries(handler: (action: string, values: AdminRequest['values']) => string): void {
  desktop.mockImplementation(async (command, arguments_) => {
    expect(command).toBe('administer')
    const request = (arguments_ as { request: AdminRequest }).request
    const operation = operations.find(item => item.id === request.operation)
    if (!operation) throw new Error(`Unknown operation ${request.operation}`)
    expect(request.confirmed).toBe(operation.write)
    return handler(operation.action, request.values)
  })
}

function inputValue(input: HTMLInputElement, value: string): void {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function role(duration: number): string {
  return `<RoleId>role-id</RoleId><RoleName>reports</RoleName><Path>/team/</Path><MaxSessionDuration>${duration}</MaxSessionDuration><AssumeRolePolicyDocument>%7B%22Statement%22%3A%5B%5D%7D</AssumeRolePolicyDocument>`
}

describe('IAM role and SNS resource workspaces', () => {
  it('keeps repeated role tags and displays the actual provider state after a partial write', async () => {
    let duration = 3600
    let tags = ['first', 'second']
    const calls: string[] = []
    mockQueries((action, values) => {
      calls.push(action)
      if (action === 'ListRoles') return query(action, `<Roles><member>${role(duration)}</member></Roles>`)
      if (action === 'GetRole') return query(action, `<Role>${role(duration)}</Role>`)
      if (action === 'ListRoleTags') return query(action, `<Tags>${tags.map(tag => `<member><Key>team</Key><Value>${tag}</Value></member>`).join('')}</Tags>`)
      if (action === 'UpdateRole') {
        duration = Number(values.MaxSessionDuration)
        return query(action)
      }
      if (action === 'UntagRole') {
        tags = []
        return query(action)
      }
      if (action === 'TagRole') {
        expect(JSON.parse(values.Tags!)).toEqual([{ key: 'team', value: 'edited' }, { key: 'team', value: 'second' }])
        throw { kind: 'storage', message: 'Tag update denied' }
      }
      throw new Error(action)
    })
    const host = mount(IamRoles, { connection: 'fixture', provider: 'ceph' })
    await vi.waitFor(() => button(host, 'Open').click())
    await vi.waitFor(() => {
      expect(Array.from(host.querySelectorAll<HTMLInputElement>('.settings-row input'), item => item.value)).toEqual(['team', 'first', 'team', 'second'])
    })
    inputValue(host.querySelector<HTMLInputElement>('input[type="number"]')!, '7200')
    inputValue(host.querySelectorAll<HTMLInputElement>('.settings-row input')[1]!, 'edited')
    host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await vi.waitFor(() => {
      expect(host.textContent).toContain('Tag update denied')
      expect(host.querySelectorAll('.settings-row input')).toHaveLength(0)
      expect(host.querySelector<HTMLInputElement>('input[type="number"]')?.value).toBe('7200')
    })
    expect(calls.slice(-5)).toEqual(['UpdateRole', 'UntagRole', 'TagRole', 'GetRole', 'ListRoleTags'])
  })

  it('keeps the Ceph managed catalog readable and exposes version availability', async () => {
    const arn = 'arn:aws:iam::aws:policy/ReadOnlyAccess'
    const policy = `<PolicyName>ReadOnlyAccess</PolicyName><Arn>${arn}</Arn><DefaultVersionId>v1</DefaultVersionId>`
    mockQueries((action) => {
      if (action === 'ListPolicies') return query(action, `<Policies><member>${policy}</member></Policies>`)
      if (action === 'GetPolicy') return query(action, `<Policy>${policy}</Policy>`)
      if (action === 'ListPolicyVersions') return query(action, '<Versions><member><VersionId>v1</VersionId><IsDefaultVersion>true</IsDefaultVersion></member></Versions>')
      throw new Error(action)
    })
    const host = mount(IamManagedPolicies, { connection: 'fixture', provider: 'ceph' })
    await vi.waitFor(() => {
      expect(host.textContent).toContain('Ceph RGW provides a fixed set of managed policies.')
      button(host, 'Open').click()
    })
    await vi.waitFor(() => {
      expect(host.textContent).toContain('Provider-managed policies are read-only.')
      expect(host.textContent).toContain('v1')
      expect(host.textContent).toContain('View JSON')
      expect(host.textContent).not.toContain('Create version')
    })
  })

  it('collapses identical topic ARNs across provider pages while preserving distinct namespaces', async () => {
    const repeated = 'arn:aws:sns:zone:tenant-a:events'
    const distinct = 'arn:aws:sns:zone:tenant-b:events'
    mockQueries((action, values) => {
      expect(action).toBe('ListTopics')
      const items = values.NextToken ? [repeated, distinct] : [repeated]
      return query(action, `<Topics>${items.map(arn => `<member><TopicArn>${arn}</TopicArn></member>`).join('')}</Topics>${values.NextToken ? '' : '<NextToken>page&amp;2</NextToken>'}`)
    })
    expect(await listTopics('fixture')).toEqual([{ arn: repeated, name: 'events' }, { arn: distinct, name: 'events' }])
    expect(desktop).toHaveBeenLastCalledWith('administer', { connection: 'fixture', request: expect.objectContaining({ values: { NextToken: 'page&2' } }) })
  })

  it('opens the bucket notification section and adds the selected ARN to a draft', async () => {
    const arn = 'arn:aws:sns:zone:tenant:events'
    desktop.mockImplementation(async (command, arguments_) => {
      expect(command).toBe('configure_bucket')
      expect(arguments_).toMatchObject({ configuration: 'notification', bucket: 'reports', change: { action: 'read' } })
      return '<NotificationConfiguration/>'
    })
    const host = mount(BucketSettings, { connection: 'fixture', bucket: 'reports', notificationTopic: arn, demo: false })
    await vi.waitFor(() => expect(button(host, 'Add selected SNS topic to draft').disabled).toBe(false))
    button(host, 'Add selected SNS topic to draft').click()
    await vi.waitFor(() => {
      expect(Array.from(host.querySelectorAll<HTMLInputElement>('input'), item => item.value)).toContain(arn)
      expect(button(host, 'Apply settings').disabled).toBe(false)
    })
    expect(desktop).toHaveBeenCalledTimes(1)
  })
})
