import type { IamPolicy } from './iam'
import { field, fields, members, policyDocument, requiredField } from './iam'

export interface RoleTag {
  key: string
  value: string
}

export interface IamRole {
  id: string
  name: string
  arn: string
  path: string
  created: string
  trust: string
  duration: number
}

export interface RoleDetails extends IamRole {
  tags: RoleTag[]
  tagError: string
}

export interface ManagedPolicy extends IamPolicy {
  description: string
  defaultVersion: string
  attachments: string
}

export interface PolicyVersion {
  id: string
  default: boolean
  created: string
}

export interface Topic {
  arn: string
  name: string
}

export interface TopicAttribute {
  name: string
  value: string
}

export function iamRole(element: Element): IamRole {
  return {
    id: requiredField(element, 'RoleId'),
    name: requiredField(element, 'RoleName'),
    arn: field(element, 'Arn'),
    path: field(element, 'Path'),
    created: field(element, 'CreateDate'),
    trust: policyDocument(field(element, 'AssumeRolePolicyDocument')),
    duration: Number(field(element, 'MaxSessionDuration') || '3600'),
  }
}

export function roleTags(element: Element): RoleTag[] {
  const rows = members(element, 'Tags')
  if (rows.length) return rows.map(row => ({ key: requiredField(row, 'Key'), value: field(row, 'Value') }))
  const container = fields(element, 'Tags')[0]
  if (!container) return []
  const keys = fields(container, 'Key')
  const values = fields(container, 'Value')
  if (keys.length !== values.length) throw new Error('The provider returned mismatched role tag keys and values.')
  return keys.map((key, index) => ({ key: field(key, 'Key') || key.textContent || '', value: values[index]?.textContent ?? '' }))
}

export function roleTagsByKey(tags: RoleTag[]): Map<string, string[]> {
  const grouped = new Map<string, string[]>()
  for (const tag of tags) grouped.set(tag.key, [...(grouped.get(tag.key) ?? []), tag.value])
  return grouped
}

export function tagsMatch(left: string[], right: string[]): boolean {
  return JSON.stringify([...left].sort()) === JSON.stringify([...right].sort())
}

export function validRoleDuration(value: number): boolean {
  return Number.isInteger(value) && value >= 3600 && value <= 43200
}

export function managedPolicy(element: Element): ManagedPolicy {
  return {
    arn: requiredField(element, 'Arn'),
    name: requiredField(element, 'PolicyName'),
    description: field(element, 'Description'),
    defaultVersion: field(element, 'DefaultVersionId'),
    attachments: field(element, 'AttachmentCount'),
  }
}

export function policyVersion(element: Element): PolicyVersion {
  return {
    id: requiredField(element, 'VersionId'),
    default: field(element, 'IsDefaultVersion').toLowerCase() === 'true',
    created: field(element, 'CreateDate'),
  }
}

export function topicFromArn(arn: string): Topic {
  return { arn, name: arn.split(':').at(-1) || arn }
}

export function topicAttributes(result: Element): TopicAttribute[] {
  const attributes = fields(result, 'Attributes')[0]
  if (!attributes) throw new Error('The provider did not return topic attributes.')
  return fields(attributes, 'entry').map(entry => ({
    name: requiredField(entry, 'key'), value: field(entry, 'value'),
  }))
}

export function knownCapabilityError(failure: unknown): boolean {
  const message = typeof failure === 'object' && failure !== null && 'message' in failure
    ? String(failure.message) : String(failure)
  return /\b(?:notimplemented|unsupportedoperation|invalidaction|not supported|not implemented|unknown operation)\b/i.test(message)
}
