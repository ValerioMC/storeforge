export type JsonValue = string | number | boolean | null | JsonValue[] | JsonObject
export interface JsonObject {
  [key: string]: JsonValue
}

export type JsonType = 'Text' | 'Number' | 'Checkbox' | 'List' | 'Object' | 'Null'

export const jsonDefaults: Record<JsonType, JsonValue> = {
  Text: '',
  Number: 0,
  Checkbox: false,
  List: [],
  Object: {},
  Null: null,
}

const statementFields: JsonObject = {
  Sid: '',
  Effect: 'Allow',
  Principal: { AWS: [] },
  NotPrincipal: { AWS: [] },
  Action: [],
  NotAction: [],
  Resource: [],
  NotResource: [],
  Condition: {},
}

const labels: Partial<Record<string, string>> = {
  Id: 'Policy ID',
  Sid: 'Statement ID',
  Statement: 'Statements',
  NotPrincipal: 'Excluded principals',
  NotAction: 'Excluded actions',
  NotResource: 'Excluded resources',
}

export function readPolicy(input: string): JsonObject {
  if (!input) return { Version: '2012-10-17', Statement: [] }
  let document: unknown
  try {
    document = JSON.parse(input)
  } catch {
    throw new Error('Choose a valid JSON bucket policy.')
  }
  if (typeof document !== 'object' || document === null || Array.isArray(document)) {
    throw new Error('A bucket policy must be a JSON object.')
  }
  return document as JsonObject
}

export function policyFields(scope: 'policy' | 'statement' | undefined): JsonObject {
  if (scope === 'policy') return { Version: '2012-10-17', Id: '', Statement: [] }
  return scope === 'statement' ? statementFields : {}
}

export function policyLabel(field: string): string {
  return Object.hasOwn(labels, field) ? labels[field]! : field
}

export function newStatement(): JsonObject {
  return { Effect: 'Allow', Principal: { AWS: [] }, Action: [], Resource: [] }
}
