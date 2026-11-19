import { desktop, errorMessage } from '../../../common/tauri'
import { administer } from './administration'
import { operations } from '../model/operations'
import {
  field, fields, iamIdentity, iamKey, iamPolicy, members, policyDocument, queryPage, queryResult, requiredField,
  type IamIdentity, type IamKey, type IamPolicy, type IdentityKind, type IssuedIamKey, type PrivateAccess,
  type PrivateAccessChange, type QueryPage,
} from '../model/iam'

export async function queryOperation(connection: string, action: string, values: Record<string, string>, confirmed = false): Promise<string> {
  const operation = operations.find(item => item.action === action && item.protocol !== 'rgw')
  if (!operation) throw new Error(`Unsupported administration action ${action}.`)
  return administer(connection, { operation: operation.id, values, confirmed })
}

export async function readQuery(connection: string, action: string, values: Record<string, string> = {}): Promise<Element> {
  return queryResult(await queryOperation(connection, action, values), action)
}

export async function writeQuery(connection: string, action: string, values: Record<string, string>): Promise<string> {
  return queryOperation(connection, action, values, true)
}

function pageFields(marker?: string): Record<string, string> {
  return marker ? { Marker: marker } : {}
}

export async function listIdentities(connection: string, kind: IdentityKind, marker?: string): Promise<QueryPage<IamIdentity>> {
  const result = await readQuery(connection, `List${kind}s`, pageFields(marker))
  return queryPage(result, members(result, `${kind}s`).map(item => iamIdentity(item, kind)))
}

export async function inspectIdentity(connection: string, kind: IdentityKind, name: string): Promise<IamIdentity> {
  const result = await readQuery(connection, `Get${kind}`, { [`${kind}Name`]: name })
  const row = fields(result, kind)[0]
  if (!row) throw new Error(`The provider did not return the selected IAM ${kind.toLowerCase()}.`)
  return iamIdentity(row, kind)
}

export async function listMembership(connection: string, kind: IdentityKind, name: string, marker?: string): Promise<QueryPage<IamIdentity>> {
  const action = kind === 'User' ? 'ListGroupsForUser' : 'GetGroup'
  const other = kind === 'User' ? 'Group' : 'User'
  const result = await readQuery(connection, action, { [`${kind}Name`]: name, ...pageFields(marker) })
  return queryPage(result, members(result, `${other}s`).map(item => iamIdentity(item, other)))
}

export async function listPolicies(connection: string, marker?: string): Promise<QueryPage<IamPolicy>> {
  const result = await readQuery(connection, 'ListPolicies', pageFields(marker))
  return queryPage(result, members(result, 'Policies').map(iamPolicy))
}

export async function listAttached(connection: string, kind: IdentityKind | 'Role', name: string, marker?: string): Promise<QueryPage<IamPolicy>> {
  const result = await readQuery(connection, `ListAttached${kind}Policies`, { [`${kind}Name`]: name, ...pageFields(marker) })
  return queryPage(result, members(result, 'AttachedPolicies').map(item => ({
    arn: requiredField(item, 'PolicyArn'), name: requiredField(item, 'PolicyName'),
  })))
}

export async function listInline(connection: string, kind: IdentityKind | 'Role', name: string, marker?: string): Promise<QueryPage<string>> {
  const result = await readQuery(connection, `List${kind}Policies`, { [`${kind}Name`]: name, ...pageFields(marker) })
  return queryPage(result, members(result, 'PolicyNames').map(item => item.textContent ?? '').filter(Boolean))
}

export async function readInline(connection: string, kind: IdentityKind | 'Role', name: string, policy: string): Promise<string> {
  const result = await readQuery(connection, `Get${kind}Policy`, { [`${kind}Name`]: name, PolicyName: policy })
  return policyDocument(field(result, 'PolicyDocument') || requiredField(result, 'Permission_policy'))
}

export async function listKeys(connection: string, user: string, marker?: string): Promise<QueryPage<IamKey>> {
  const result = await readQuery(connection, 'ListAccessKeys', { UserName: user, ...pageFields(marker) })
  return queryPage(result, members(result, 'AccessKeyMetadata').map(iamKey))
}

export async function issueKey(connection: string, user: string): Promise<IssuedIamKey> {
  const result = queryResult(await writeQuery(connection, 'CreateAccessKey', { UserName: user }), 'CreateAccessKey')
  const row = fields(result, 'AccessKey')[0]
  if (!row) throw new Error('The provider did not return the issued access key.')
  const owner = requiredField(row, 'UserName')
  if (owner.toLowerCase() !== user.toLowerCase()) throw new Error('The provider returned a credential for a different user.')
  return { ...iamKey(row), owner, secret: requiredField(row, 'SecretAccessKey') }
}

export interface IdentityDraft {
  name: string
  path: string
  members: string[]
  policies: string[]
}

export interface CreatedIdentity {
  identity: IamIdentity
  failures: string[]
}

export async function createIdentity(connection: string, kind: IdentityKind, draft: IdentityDraft): Promise<CreatedIdentity> {
  const action = `Create${kind}`
  const name = draft.name
  const result = queryResult(await writeQuery(connection, action, { [`${kind}Name`]: name, Path: draft.path }), action)
  const resource = fields(result, kind)[0]
  const identity = resource ? iamIdentity(resource, kind) : await inspectIdentity(connection, kind, name)
  const failures: string[] = []
  for (const member of draft.members) {
    const values = kind === 'User' ? { UserName: identity.name, GroupName: member } : { UserName: member, GroupName: identity.name }
    await applyAttachment(connection, 'AddUserToGroup', values, `Membership for ${member}`, failures)
  }
  for (const policy of draft.policies) {
    await applyAttachment(connection, `Attach${kind}Policy`, { [`${kind}Name`]: identity.name, PolicyArn: policy }, policy, failures)
  }
  try {
    return { identity: await inspectIdentity(connection, kind, identity.name), failures }
  } catch (failure) {
    return { identity, failures: [...failures, `Refresh: ${errorMessage(failure)}`] }
  }
}

async function applyAttachment(connection: string, action: string, values: Record<string, string>, label: string, failures: string[]): Promise<void> {
  try {
    await writeQuery(connection, action, values)
  } catch (failure) {
    failures.push(`${label}: ${errorMessage(failure)}`)
  }
}

export function listPrivateAccess(connection: string): Promise<PrivateAccess[]> {
  return desktop('list_private_iam', { connection })
}

export function changePrivateAccess(connection: string, change: PrivateAccessChange): Promise<PrivateAccess | null> {
  return desktop('change_private_iam', { connection, change })
}
