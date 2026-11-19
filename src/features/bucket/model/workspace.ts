import { errorMessage, isMissing } from '../../../common/tauri'
import type { Configuration } from './bucket'
import { configurations } from './configuration'
import { readPolicy } from './policy-document'
import { readXml, writeXml, xmlChildren, setXmlValue } from './xml-document'
import { xmlSettings } from './xml-settings'

export const configurationGroups = [
  { label: 'Properties', sections: ['versioning', 'encryption', 'objectLock', 'tagging', 'logging'] },
  { label: 'Permissions', sections: ['policy', 'acl', 'ownershipControls', 'publicAccessBlock', 'cors'] },
  { label: 'Advanced', sections: ['lifecycle', 'notification', 'replication', 'website', 'requestPayment'] },
] as const

export const ruleFeatures = [
  { value: 'lifecycle', label: 'Lifecycle' },
  { value: 'policy', label: 'Policy' },
  { value: 'cors', label: 'CORS' },
  { value: 'notification', label: 'Topics' },
  { value: 'tagging', label: 'Tags' },
] as const

export type RuleFeature = typeof ruleFeatures[number]['value']
export type ConfigurationReading =
  | { status: 'configured' | 'missing'; document: string; label: string }
  | { status: 'error'; message: string }
export type ConfigurationReader = (bucket: string, configuration: Configuration) => Promise<string>

export interface BucketRule {
  name: string
  detail: string
  status: string
  raw: string
}

export function configurationLabel(configuration: Configuration): string {
  return configurations.find(option => option.id === configuration)!.label
}

export function parseConfiguration(configuration: Configuration, source: string) {
  if (configuration === 'policy') return { policy: readPolicy(source), xml: undefined }
  return { xml: readXml(source, xmlSettings[configuration].root), policy: undefined }
}

export function sectionStatus(configuration: Configuration, source: string): string {
  if (!source) return 'Not configured'
  if (configuration === 'policy') {
    const { Statement } = readPolicy(source)
    return Statement && (!Array.isArray(Statement) || Statement.length) ? 'Configured' : 'Not configured'
  }
  const root = readXml(source, xmlSettings[configuration].root).documentElement
  const flag = {
    versioning: 'Status', objectLock: 'ObjectLockEnabled', requestPayment: 'Payer',
  } as Partial<Record<Configuration, string>>
  const status = flag[configuration]
  if (status) return textOf(root, status) || 'Not configured'
  if (configuration === 'publicAccessBlock') {
    return Array.from(root.children).some(child => child.namespaceURI === root.namespaceURI && child.textContent === 'true')
      ? 'Enabled' : 'Disabled'
  }
  const container = configuration === 'tagging' ? xmlChildren(root, 'TagSet')[0] : root
  return container && Array.from(container.children).some(child => child.namespaceURI === root.namespaceURI)
    ? 'Configured' : 'Not configured'
}

export async function readConfiguration(read: ConfigurationReader, bucket: string, configuration: Configuration): Promise<ConfigurationReading> {
  try {
    const document = await read(bucket, configuration)
    const label = sectionStatus(configuration, document)
    return { status: label === 'Not configured' ? 'missing' : 'configured', document, label }
  } catch (failure) {
    return isMissing(failure) ? { status: 'missing', document: '', label: 'Not configured' } : { status: 'error', message: errorMessage(failure) }
  }
}

export async function scanConfigurations(
  buckets: readonly string[],
  sections: readonly Configuration[],
  read: ConfigurationReader,
  receive: (bucket: string, section: Configuration, reading: ConfigurationReading) => void,
  current: () => boolean,
): Promise<void> {
  const requests = buckets.flatMap(bucket => sections.map(section => ({ bucket, section })))
  let position = 0
  async function next(): Promise<void> {
    while (current()) {
      const request = requests[position++]
      if (!request) return
      const result = await readConfiguration(read, request.bucket, request.section)
      if (current()) receive(request.bucket, request.section, result)
    }
  }
  await Promise.all(Array.from({ length: Math.min(6, requests.length) }, next))
}

function textOf(element: Element, name: string): string {
  return xmlChildren(element, name)[0]?.textContent ?? ''
}

export function extractRules(feature: RuleFeature, source: string): BucketRule[] {
  if (!source) return []
  if (feature === 'policy') {
    const statements = readPolicy(source).Statement
    return (Array.isArray(statements) ? statements : [statements]).flatMap((rule, index) => {
      if (!rule || typeof rule !== 'object' || Array.isArray(rule)) return []
      return [{
        name: typeof rule.Sid === 'string' && rule.Sid ? rule.Sid : `Statement ${index + 1}`,
        detail: Array.isArray(rule.Action) ? rule.Action.join(', ') : String(rule.Action ?? rule.NotAction ?? ''),
        status: typeof rule.Effect === 'string' ? rule.Effect : '',
        raw: JSON.stringify(rule, null, 2),
      }]
    })
  }
  const root = readXml(source, xmlSettings[feature].root).documentElement
  const elements = feature === 'tagging'
    ? xmlChildren(root, 'TagSet').flatMap(set => xmlChildren(set, 'Tag'))
    : xmlChildren(root, { lifecycle: 'Rule', cors: 'CORSRule', notification: 'TopicConfiguration' }[feature])
  return elements.map((element, index) => ({
    name: textOf(element, feature === 'tagging' ? 'Key' : feature === 'cors' || feature === 'notification' ? 'Id' : 'ID') || `Rule ${index + 1}`,
    detail: ruleDetail(feature, element),
    status: textOf(element, 'Status'),
    raw: new XMLSerializer().serializeToString(element),
  }))
}

function ruleDetail(feature: Exclude<RuleFeature, 'policy'>, element: Element): string {
  if (feature === 'tagging') return textOf(element, 'Value')
  if (feature === 'notification') return textOf(element, 'Topic')
  if (feature === 'cors') return xmlChildren(element, 'AllowedOrigin').map(origin => origin.textContent).join(', ')
  const filter = xmlChildren(element, 'Filter')[0]
  const and = filter && xmlChildren(filter, 'And')[0]
  return textOf(and ?? filter ?? element, 'Prefix')
}

export function setLifecycleRuleStatus(source: string, index: number, status: 'Enabled' | 'Disabled'): string {
  const root = readXml(source, xmlSettings.lifecycle.root).documentElement
  const rule = xmlChildren(root, 'Rule')[index]
  if (!rule) throw new Error('The lifecycle rule is unavailable. Reload the configuration.')
  setXmlValue(rule, { kind: 'select', name: 'Status', label: 'Status', options: [] }, status, xmlChildren(rule, 'Status')[0])
  return writeXml(root)
}

export interface BucketBackup {
  format: 'storeforge-bucket-config'
  version: 1
  bucket: string
  documents: Partial<Record<Configuration, string>>
}

export function readBackup(source: string): BucketBackup {
  const value: unknown = JSON.parse(source)
  if (!value || typeof value !== 'object' || !('format' in value) || value.format !== 'storeforge-bucket-config' ||
    !('version' in value) || value.version !== 1 || !('bucket' in value) || typeof value.bucket !== 'string' ||
    !('documents' in value) || !value.documents || typeof value.documents !== 'object' || Array.isArray(value.documents)) {
    throw new Error('Choose a StoreForge bucket configuration backup.')
  }
  const documents: BucketBackup['documents'] = {}
  for (const [section, source] of Object.entries(value.documents)) {
    const configuration = configurations.find(option => option.id === section)?.id
    if (!configuration || typeof source !== 'string' || !source.trim()) throw new Error(`Invalid backup section: ${section}.`)
    parseConfiguration(configuration, source)
    documents[configuration] = source
  }
  if (!Object.keys(documents).length) throw new Error('The backup has no configuration documents.')
  return { format: value.format, version: value.version, bucket: value.bucket, documents }
}
