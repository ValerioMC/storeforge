import { errorMessage } from '../../../common/tauri'
import type { Provider } from '../../connection/model/connection'
import { readQuery, writeQuery } from './iam'
import { fields, members, policyDocument, queryPage, queryResult, requiredField, type QueryPage } from '../model/iam'
import {
  iamRole, knownCapabilityError, managedPolicy, policyVersion, roleTags, roleTagsByKey, tagsMatch, topicAttributes, topicFromArn,
  type IamRole, type ManagedPolicy, type PolicyVersion, type RoleDetails, type RoleTag, type Topic, type TopicAttribute,
} from '../model/resources'

export interface RoleDraft {
  name: string
  path: string
  trust: string
  duration: number
}

export async function listRoles(connection: string, marker?: string): Promise<QueryPage<IamRole>> {
  const result = await readQuery(connection, 'ListRoles', marker ? { Marker: marker } : {})
  return queryPage(result, members(result, 'Roles').map(iamRole))
}

export async function inspectRole(connection: string, name: string): Promise<RoleDetails> {
  const result = await readQuery(connection, 'GetRole', { RoleName: name })
  const resource = requiredElement(result, 'Role')
  const role = iamRole(resource)
  try {
    const tags = fields(resource, 'Tags').length ? roleTags(resource) : await collectPages(async marker => {
      const response = await readQuery(connection, 'ListRoleTags', { RoleName: name, ...(marker ? { Marker: marker } : {}) })
      return queryPage(response, roleTags(response))
    })
    return { ...role, tags, tagError: '' }
  } catch (failure) {
    const tagError = knownCapabilityError(failure) ? 'This IAM endpoint does not support role tags.' : errorMessage(failure)
    return { ...role, tags: [], tagError }
  }
}

export async function createRole(connection: string, draft: RoleDraft): Promise<string> {
  await writeQuery(connection, 'CreateRole', {
    RoleName: draft.name, Path: draft.path, AssumeRolePolicyDocument: draft.trust, MaxSessionDuration: String(draft.duration),
  })
  return draft.name
}

export async function updateRole(
  connection: string,
  provider: Provider,
  baseline: RoleDetails,
  draft: { duration: number; tags: RoleTag[] },
): Promise<void> {
  const role = { RoleName: baseline.name }
  if (draft.duration !== baseline.duration) await writeQuery(connection, 'UpdateRole', { ...role, MaxSessionDuration: String(draft.duration) })
  const previous = roleTagsByKey(baseline.tags)
  const desired = roleTagsByKey(draft.tags)
  const changes = [...new Set([...previous.keys(), ...desired.keys()])].map(key => ({
    key, before: previous.get(key) ?? [], after: desired.get(key) ?? [],
  })).filter(change => !tagsMatch(change.before, change.after))
  changes.sort((left, right) => (left.after.length - left.before.length) - (right.after.length - right.before.length))
  for (const { key, before, after } of changes) {
    if (before.length && (provider !== 'aws' || !after.length)) {
      await writeQuery(connection, 'UntagRole', { ...role, 'TagKeys.member.1': key })
    }
    if (after.length) {
      const tags = after.map(value => ({ key, value }))
      await writeQuery(connection, 'TagRole', { ...role, Tags: JSON.stringify(tags) })
    }
  }
}

export async function listManagedPolicies(connection: string, marker?: string): Promise<QueryPage<ManagedPolicy>> {
  const result = await readQuery(connection, 'ListPolicies', marker ? { Marker: marker } : {})
  return queryPage(result, members(result, 'Policies').map(managedPolicy))
}

export async function inspectManagedPolicy(connection: string, arn: string): Promise<ManagedPolicy> {
  const result = await readQuery(connection, 'GetPolicy', { PolicyArn: arn })
  return managedPolicy(requiredElement(result, 'Policy'))
}

export async function listPolicyVersions(connection: string, arn: string, marker?: string): Promise<QueryPage<PolicyVersion>> {
  const result = await readQuery(connection, 'ListPolicyVersions', { PolicyArn: arn, ...(marker ? { Marker: marker } : {}) })
  return queryPage(result, members(result, 'Versions').map(policyVersion))
}

export async function readPolicyVersion(connection: string, arn: string, id: string): Promise<string> {
  const result = await readQuery(connection, 'GetPolicyVersion', { PolicyArn: arn, VersionId: id })
  return policyDocument(requiredField(requiredElement(result, 'PolicyVersion'), 'Document'))
}

export async function createManagedPolicy(connection: string, name: string, document: string, description: string): Promise<ManagedPolicy> {
  const result = queryResult(await writeQuery(connection, 'CreatePolicy', { PolicyName: name, PolicyDocument: document, Description: description }), 'CreatePolicy')
  return managedPolicy(requiredElement(result, 'Policy'))
}

export async function listTopics(connection: string): Promise<Topic[]> {
  const topics = await collectPages(async marker => {
    const result = await readQuery(connection, 'ListTopics', marker ? { NextToken: marker } : {})
    return queryPage(result, members(result, 'Topics').map(row => topicFromArn(requiredField(row, 'TopicArn'))), 'NextToken')
  })
  const topicsByArn = new Map(topics.map(topic => [topic.arn, topic]))
  return [...topicsByArn.values()]
}

export async function inspectTopic(connection: string, arn: string): Promise<TopicAttribute[]> {
  return topicAttributes(await readQuery(connection, 'GetTopicAttributes', { TopicArn: arn }))
}

export async function createTopic(connection: string, name: string, attribute?: TopicAttribute): Promise<Topic> {
  const values: Record<string, string> = attribute ? {
    Name: name, 'Attributes.entry.1.key': attribute.name, 'Attributes.entry.1.value': attribute.value,
  } : { Name: name }
  const response = queryResult(await writeQuery(connection, 'CreateTopic', values), 'CreateTopic')
  return topicFromArn(requiredField(response, 'TopicArn'))
}

export async function setTopicAttribute(connection: string, arn: string, attribute: TopicAttribute): Promise<void> {
  await writeQuery(connection, 'SetTopicAttributes', { TopicArn: arn, AttributeName: attribute.name, AttributeValue: attribute.value })
}

function requiredElement(element: Element, name: string): Element {
  const child = fields(element, name)[0]
  if (!child) throw new Error(`The provider response is missing ${name}.`)
  return child
}

async function collectPages<T>(read: (marker?: string) => Promise<QueryPage<T>>): Promise<T[]> {
  const items: T[] = []
  const visited = new Set<string>()
  let marker: string | undefined
  do {
    const page = await read(marker)
    items.push(...page.items)
    marker = page.marker ?? undefined
    if (marker && visited.has(marker)) throw new Error('The provider repeated a page marker.')
    if (marker) visited.add(marker)
  } while (marker)
  return items
}

export function snsError(failure: unknown): string {
  return knownCapabilityError(failure) ? 'This endpoint does not support the requested SNS capability.' : errorMessage(failure)
}

export function topicPolicy(attributes: TopicAttribute[]): string {
  return attributes.find(item => item.name === 'Policy')?.value ?? ''
}
