export interface QueryPage<T> {
  items: T[]
  marker: string | null
}

export interface IamIdentity {
  id: string
  name: string
  path: string
  arn: string
  created: string
}

export interface IamPolicy {
  arn: string
  name: string
}

export interface IamKey {
  id: string
  status: 'Active' | 'Inactive'
  created: string
}

export interface IssuedIamKey extends IamKey {
  owner: string
  secret: string
}

export interface InlinePolicy {
  name: string
  document: string
}

export interface PrivateAccessDraft {
  name: string
  browser: boolean
  manager: boolean
  groups: string[]
  policies: string[]
  inlinePolicies: InlinePolicy[]
}

export interface PrivateAccess {
  id: string
  name: string
  userName: string
  status: 'provisioning' | 'active' | 'setupIncomplete' | 'retiring'
  issue?: string | null
}

export type PrivateAccessChange =
  | { action: 'create'; draft: PrivateAccessDraft }
  | { action: 'retry' | 'retire'; id: string }

export const identityKinds = ['User', 'Group'] as const
export type IdentityKind = typeof identityKinds[number]

export function fields(element: Element, name: string): Element[] {
  return Array.from(element.children).filter(child => child.localName === name)
}

export function field(element: Element, name: string): string {
  return fields(element, name)[0]?.textContent ?? ''
}

export function requiredField(element: Element, name: string): string {
  const value = field(element, name)
  if (!value) throw new Error(`The provider response is missing ${name}.`)
  return value
}

export function queryResult(response: string, action: string): Element {
  const root = new DOMParser().parseFromString(response, 'application/xml').documentElement
  const result = fields(root, `${action}Result`)[0]
  if (root.localName !== `${action}Response` || !result) {
    throw new Error(`The provider returned an unexpected ${action} response.`)
  }
  return result
}

export function members(element: Element, name: string): Element[] {
  const container = fields(element, name)[0]
  return container ? fields(container, 'member') : []
}

export function queryPage<T>(result: Element, list: T[], token = 'Marker'): QueryPage<T> {
  const marker = field(result, token)
  if (field(result, 'IsTruncated').toLowerCase() === 'true' && !marker) {
    throw new Error('The provider returned a truncated result without a page marker.')
  }
  return { items: list, marker: marker || null }
}

export function iamIdentity(element: Element, kind: IdentityKind): IamIdentity {
  return {
    id: requiredField(element, `${kind}Id`),
    name: requiredField(element, `${kind}Name`),
    path: field(element, 'Path'),
    arn: field(element, 'Arn'),
    created: field(element, 'CreateDate'),
  }
}

export function iamPolicy(element: Element): IamPolicy {
  return { arn: requiredField(element, 'Arn'), name: requiredField(element, 'PolicyName') }
}

export function iamKey(element: Element): IamKey {
  const status = requiredField(element, 'Status')
  if (status !== 'Active' && status !== 'Inactive') throw new Error('The provider returned an unknown access key status.')
  return { id: requiredField(element, 'AccessKeyId'), status, created: field(element, 'CreateDate') }
}

export function policyDocument(value: string): string {
  if (value.trimStart().startsWith('{')) return value
  return decodeURIComponent(value)
}

export function managedUser(name: string): boolean {
  return name.toLowerCase().startsWith('storeforge-private-')
}

export function validPolicy(document: string): boolean {
  try {
    const parsed: unknown = JSON.parse(document)
    return parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)
  } catch {
    return false
  }
}

export function keyAnnotationId(connection: string, userId: string, key: string): string {
  return JSON.stringify(['storeforge.iam.key', connection, userId, key])
}
