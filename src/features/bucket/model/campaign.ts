import { errorMessage, isUnsupported } from '../../../common/tauri'
import type { BucketUsage, Reading } from '../../monitoring/model/monitoring'
import type { BucketQuota, Configuration } from './bucket'
import { configurations } from './configuration'
import { readPolicy, type JsonValue } from './policy-document'
import { configurationLabel, parseConfiguration, readBackup, sectionStatus } from './workspace'
import { readWorkbenchFeature, type WorkbenchRow } from './workbench'
import { readXml, writeXml, xmlChildren } from './xml-document'

export const campaignConfigurations = ['versioning', 'lifecycle', 'notification', 'publicAccessBlock', 'cors', 'policy'] as const
export type CampaignConfiguration = typeof campaignConfigurations[number]
export type CampaignSection = CampaignConfiguration | 'quota'

export const campaignSections: { value: CampaignSection; label: string }[] = [
  ...campaignConfigurations.map(value => ({ value, label: configurationLabel(value) })),
  { value: 'quota', label: 'Ceph quota' },
]

export interface CampaignPatch {
  sections: ConfigIntent[]
  quota?: Partial<BucketQuota>
  usage?: boolean
}

export interface ConfigIntent {
  section: CampaignConfiguration
  operation: 'merge' | 'replace' | 'remove'
  document: string
  removeIds?: string[]
}

export interface ConfigChange {
  configuration: CampaignConfiguration
  expected: string
  change: { action: 'write'; document: string } | { action: 'remove' }
}

export interface CampaignSnapshot {
  name: string
  documents: Partial<Record<CampaignConfiguration, string>>
  quota?: BucketQuota
}

type CampaignAction =
  | { kind: 'configuration'; update: ConfigChange }
  | { kind: 'quota'; expected: BucketQuota; draft: BucketQuota }
  | { kind: 'usage' }

export interface CampaignItem {
  section: CampaignSection | 'usage'
  status: 'ready' | 'unchanged' | 'unsupported' | 'denied' | 'error' | 'applied' | 'partial' | 'skipped'
  summary: string
  action?: CampaignAction
  before?: string
  after?: string
  quota?: BucketQuota
  usage?: Reading<BucketUsage>
}

export interface CampaignTarget {
  row: WorkbenchRow
  patch: CampaignPatch
  items: CampaignItem[]
}

export interface CampaignGateway {
  read(row: WorkbenchRow, section: Configuration): Promise<string>
  quota(row: WorkbenchRow): Promise<BucketQuota>
  configure(row: WorkbenchRow, update: ConfigChange): Promise<string>
  updateQuota(row: WorkbenchRow, expected: BucketQuota, draft: BucketQuota): Promise<BucketQuota>
  usage(row: WorkbenchRow): Promise<Reading<BucketUsage>>
}

export function campaignItemLabel(section: CampaignItem['section']): string {
  if (section === 'usage') return 'Bucket usage'
  return section === 'quota' ? 'Ceph quota' : configurationLabel(section)
}

export const campaignStatuses: Record<CampaignItem['status'], string> = {
  ready: 'Ready', unchanged: 'No change', unsupported: 'Unsupported', denied: 'Denied', error: 'Failed',
  applied: 'Applied', partial: 'Partial', skipped: 'Skipped',
}

export function isRuleSection(section: CampaignConfiguration): section is 'lifecycle' | 'notification' {
  return section === 'lifecycle' || section === 'notification'
}

export function exampleDocument(section: CampaignConfiguration): string {
  return configurations.find(option => option.id === section)!.example
}

export function validateConfigIntent(intent: ConfigIntent): void {
  const { section, operation, document, removeIds } = intent
  if (operation === 'merge' && isRuleSection(section) && removeIds?.length && !document.trim()) return
  if (operation === 'remove') {
    if (section === 'versioning') throw new Error('Suspend versioning by choosing Suspended.')
    return
  }
  if (!document.trim()) throw new Error(`Enter the ${configurationLabel(section).toLowerCase()} document.`)
  parseConfiguration(section, document)
  if (section === 'versioning' && !['Enabled', 'Suspended'].includes(versioningStatus(document))) throw new Error('Choose Enabled or Suspended.')
}

export function snapshotPatch(
  source: CampaignSnapshot, sections: readonly CampaignSection[], target: WorkbenchRow,
): CampaignPatch {
  const configurations: ConfigIntent[] = []
  for (const section of campaignConfigurations) {
    const original = source.documents[section]
    if (!sections.includes(section) || !original) continue
    const document = section === 'policy' ? rebindPolicy(original, source.name, target.name) : original
    configurations.push({ section, operation: isRuleSection(section) ? 'merge' : 'replace', document })
  }
  return { sections: configurations, quota: sections.includes('quota') ? source.quota : undefined }
}

function rebindPolicy(document: string, source: string, target: string): string {
  if (source === target) return document
  function replace(value: JsonValue): JsonValue {
    if (Array.isArray(value)) return value.map(replace)
    if (typeof value !== 'string') return value
    const resource = /^(arn:[^:]+:s3:::)([^/]+)(\/.*)?$/.exec(value)
    return resource?.[2] === source ? `${resource[1]}${target}${resource[3] ?? ''}` : value
  }
  const policy = readPolicy(document)
  const statements = Array.isArray(policy.Statement) ? policy.Statement : [policy.Statement]
  for (const statement of statements) {
    if (!statement || typeof statement !== 'object' || Array.isArray(statement)) continue
    for (const field of ['Resource', 'NotResource']) {
      const value = statement[field]
      if (value !== undefined) statement[field] = replace(value)
    }
  }
  return JSON.stringify(policy, null, 2)
}

export async function captureCampaign(
  row: WorkbenchRow, sections: readonly CampaignSection[], gateway: CampaignGateway,
): Promise<{ snapshot: CampaignSnapshot; items: CampaignItem[] }> {
  const snapshot: CampaignSnapshot = { name: row.name, documents: {} }
  const items: CampaignItem[] = []
  for (const section of sections) {
    if (section === 'quota') {
      if (!row.quotaAvailable) {
        items.push({ section, status: 'unsupported', summary: 'This context has no privileged quota access.' })
        continue
      }
      try {
        snapshot.quota = await gateway.quota(row)
        items.push({ section, status: 'applied', summary: 'Available to copy.' })
      } catch (failure) {
        items.push(failedItem(section, failure))
      }
      continue
    }
    const reading = await readWorkbenchFeature(row, section, () => gateway.read(row, section))
    if (reading.status === 'configured') {
      snapshot.documents[section] = reading.document
      items.push({ section, status: 'applied', summary: 'Available to copy.' })
    } else if (reading.status === 'unsupported' || reading.status === 'error') {
      items.push(unavailableConfiguration(section, reading))
    } else {
      items.push({ section, status: 'unchanged', summary: 'Unconfigured at source.' })
    }
  }
  return { snapshot, items }
}

function unavailableConfiguration(
  section: CampaignConfiguration, reading: { status: 'unsupported' | 'error'; message: string },
): CampaignItem {
  return reading.status === 'unsupported'
    ? { section, status: 'unsupported', summary: reading.message }
    : { section, status: failureStatus(reading.message), summary: reading.message }
}

export function writeCampaignBackup(source: CampaignSnapshot): string {
  return JSON.stringify({ format: 'storeforge-bucket-campaign', version: 1, bucket: source.name, documents: source.documents, quota: source.quota }, null, 2)
}

export function readCampaignBackup(text: string): CampaignSnapshot {
  const value: unknown = JSON.parse(text)
  if (!value || typeof value !== 'object' || !('format' in value)) throw new Error('Choose a StoreForge bucket backup.')
  if (value.format === 'storeforge-bucket-config') {
    const backup = readBackup(text)
    return { name: backup.bucket, documents: eligibleDocuments(backup.documents) }
  }
  if (value.format !== 'storeforge-bucket-campaign' || !('version' in value) || value.version !== 1 ||
    !('bucket' in value) || typeof value.bucket !== 'string' || !('documents' in value) ||
    !value.documents || typeof value.documents !== 'object' || Array.isArray(value.documents)) {
    throw new Error('Choose a StoreForge bucket campaign backup.')
  }
  const documents = eligibleDocuments(Object.fromEntries(Object.entries(value.documents).map(([section, document]) => {
    if (typeof document !== 'string') throw new Error(`Invalid configuration document: ${section}.`)
    return [section, document]
  })))
  const quota = 'quota' in value ? validateQuota(value.quota) : undefined
  if (!snapshotSections({ name: value.bucket, documents, quota }).length) throw new Error('The backup has no supported campaign settings.')
  return { name: value.bucket, documents, quota }
}

function eligibleDocuments(documents: Partial<Record<string, string>>): CampaignSnapshot['documents'] {
  const result: CampaignSnapshot['documents'] = {}
  for (const section of campaignConfigurations) {
    const document = documents[section]
    if (!document) continue
    validateConfigIntent({ section, operation: 'replace', document })
    result[section] = document
  }
  return result
}

export function snapshotSections(snapshot: CampaignSnapshot): CampaignSection[] {
  return campaignSections.map(({ value }) => value).filter(section => section === 'quota' ? !!snapshot.quota : !!snapshot.documents[section])
}

function validateQuota(value: unknown): BucketQuota {
  if (!value || typeof value !== 'object' || !('enabled' in value) || typeof value.enabled !== 'boolean' ||
    !('maxBytes' in value) || !('maxObjects' in value) || typeof value.maxBytes !== 'string' || typeof value.maxObjects !== 'string') {
    throw new Error('Invalid bucket quota.')
  }
  return { enabled: value.enabled, maxBytes: quotaLimit(value.maxBytes), maxObjects: quotaLimit(value.maxObjects) }
}

export function quotaLimit(value: string): string {
  if (!value) return ''
  if (!/^[0-9]+$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error('Enter whole quota limits within the provider’s integer range.')
  return String(BigInt(value))
}

function quotaText(quota: BucketQuota): string {
  return `${quota.enabled ? 'Enabled' : 'Disabled'} · ${quota.maxBytes || 'Unlimited'} bytes · ${quota.maxObjects || 'Unlimited'} objects`
}

function failureStatus(message: string): 'denied' | 'error' {
  return /\b(403|AccessDenied|Forbidden)\b|was denied/i.test(message) ? 'denied' : 'error'
}

function failedItem(section: CampaignItem['section'], failure: unknown): CampaignItem {
  const summary = errorMessage(failure)
  return { section, status: isUnsupported(failure) ? 'unsupported' : failureStatus(summary), summary }
}

export async function previewCampaignTarget(row: WorkbenchRow, patch: CampaignPatch, gateway: CampaignGateway): Promise<CampaignTarget> {
  const items: CampaignItem[] = []
  for (const intent of patch.sections) items.push(await previewConfiguration(row, intent, gateway))
  if (patch.quota) items.push(await previewQuota(row, patch.quota, gateway))
  if (patch.usage) {
    items.push(row.configurationAvailable
      ? { section: 'usage', status: 'ready', summary: 'Scan the selected bucket and record current usage and coverage.', action: { kind: 'usage' } }
      : { section: 'usage', status: 'unsupported', summary: 'This context has no S3 listing identity for this bucket.' })
  }
  return { row, patch, items }
}

async function previewConfiguration(row: WorkbenchRow, intent: ConfigIntent, gateway: CampaignGateway): Promise<CampaignItem> {
  const { section } = intent
  const reading = await readWorkbenchFeature(row, section, () => gateway.read(row, section))
  if (reading.status === 'unsupported' || reading.status === 'error') return unavailableConfiguration(section, reading)
  try {
    const expected = reading.document
    const plan = configurationChange(intent, expected)
    if (!plan) return { section, status: 'unchanged', summary: reading.status === 'missing' ? 'Unconfigured at target.' : 'Matches current configuration.' }
    const before = reading.status === 'missing' ? 'Unconfigured' : expected
    const after = plan.change.action === 'remove' ? 'Unconfigured' : plan.change.document
    return { section, status: 'ready', before, after, summary: plan.summary,
      action: { kind: 'configuration', update: { configuration: section, expected, change: plan.change } } }
  } catch (failure) {
    return failedItem(section, failure)
  }
}

async function previewQuota(row: WorkbenchRow, changes: Partial<BucketQuota>, gateway: CampaignGateway): Promise<CampaignItem> {
  if (!row.quotaAvailable) return { section: 'quota', status: 'unsupported', summary: 'This context has no privileged quota access.' }
  try {
    const expected = await gateway.quota(row)
    const draft = validateQuota({ ...expected, ...changes })
    if (sameQuota(expected, draft)) return { section: 'quota', status: 'unchanged', summary: 'Matches current quota.' }
    return { section: 'quota', status: 'ready', summary: 'Update selected quota fields.', before: quotaText(expected), after: quotaText(draft),
      action: { kind: 'quota', expected, draft } }
  } catch (failure) {
    return failedItem('quota', failure)
  }
}

function sameQuota(left: BucketQuota, right: BucketQuota): boolean {
  return left.enabled === right.enabled && left.maxBytes === right.maxBytes && left.maxObjects === right.maxObjects
}

export async function applyCampaignItem(row: WorkbenchRow, item: CampaignItem, gateway: CampaignGateway): Promise<CampaignItem> {
  if (item.status !== 'ready' || !item.action) return item
  if (row.profile.id === 'demo') return { ...item, status: 'unsupported', summary: 'This demo is read-only.' }
  try {
    const action = item.action
    if (action.kind === 'quota') {
      const quota = await gateway.updateQuota(row, action.expected, action.draft)
      if (!sameQuota(quota, action.draft)) throw new Error('The provider returned a different quota after applying.')
      return { ...item, status: 'applied', quota }
    }
    if (action.kind === 'configuration') {
      const current = await gateway.configure(row, action.update)
      if (!matchesReadback(action.update, current)) throw new Error('The provider returned a different configuration after applying.')
      return { ...item, status: 'applied' }
    }
    const usage = await gateway.usage(row)
    if (!usage.value) return { ...item, status: usage.coverage === 'denied' ? 'denied' : 'error', summary: usage.detail ?? 'The provider did not return usage.' }
    const status = usage.coverage === 'partial' ? 'partial' : 'applied'
    return { ...item, status, usage, summary: [`${usage.value.currentBytes?.toLocaleString() ?? 'Unknown'} bytes`,
      `${usage.value.currentObjects?.toLocaleString() ?? 'unknown'} objects`, usage.detail].filter(Boolean).join(' · ') }
  } catch (failure) {
    return { ...item, ...failedItem(item.section, failure) }
  }
}

function matchesReadback(update: ConfigChange, current: string): boolean {
  const { configuration, change } = update
  if (change.action === 'remove') return sectionStatus(configuration, current) === 'Not configured'
  if (configuration === 'versioning') return versioningStatus(current) === versioningStatus(change.document)
  return equivalentConfiguration(configuration, current, change.document)
}

function versioningStatus(document: string): string {
  return xmlChildren(readXml(document, 'VersioningConfiguration').documentElement, 'Status')[0]?.textContent ?? ''
}

function configurationChange(intent: ConfigIntent, expected: string): { change: ConfigChange['change']; summary: string } | undefined {
  const { section, operation, document } = intent
  if (operation === 'remove') {
    if (sectionStatus(section, expected) === 'Not configured' || !expected) return undefined
    const option = configurations.find(candidate => candidate.id === section)
    if (option?.remove) return { change: { action: 'remove' }, summary: 'Remove the current configuration.' }
    if (section === 'notification') return { change: { action: 'write', document: writeXml(readXml('', 'NotificationConfiguration').documentElement) },
      summary: 'Remove the current notifications.' }
    throw new Error('This configuration must be updated using a document.')
  }

  let desired = document
  let summary = sectionStatus(section, expected) === 'Not configured' ? 'Configure this setting.' : 'Update the current configuration.'
  if (section === 'versioning') {
    const status = versioningStatus(document)
    if (versioningStatus(expected) === status) return undefined
    const root = readXml(expected, 'VersioningConfiguration').documentElement
    const node = xmlChildren(root, 'Status')[0] ?? root.appendChild(root.ownerDocument.createElementNS(root.namespaceURI, 'Status'))
    node.textContent = status
    desired = writeXml(root)
    summary = `Set versioning to ${status}.`
  } else if (operation === 'merge' && isRuleSection(section)) {
    const merged = mergeRules(section, expected, document, intent.removeIds ?? [])
    if (!merged) return undefined
    const { root, added, updated, removed } = merged
    summary = `${added} added · ${updated} updated · ${removed} removed`
    if (section === 'lifecycle' && !xmlChildren(root, 'Rule').length) return { change: { action: 'remove' }, summary }
    desired = writeXml(root)
  }
  if (expected && equivalentConfiguration(section, expected, desired)) return undefined
  return { change: { action: 'write', document: desired }, summary }
}

const ruleSettings = {
  lifecycle: { root: 'LifecycleConfiguration', elements: ['Rule'], id: 'ID' },
  notification: { root: 'NotificationConfiguration', elements: ['TopicConfiguration', 'QueueConfiguration', 'CloudFunctionConfiguration', 'EventBridgeConfiguration'], id: 'Id' },
} as const

function mergeRules(section: keyof typeof ruleSettings, current: string, incoming: string, removeIds: string[]) {
  const schema = ruleSettings[section]
  const root = readXml(current, schema.root).documentElement
  const source = readXml(incoming, schema.root).documentElement
  const wanted = rulesById(source, schema.elements, schema.id)
  const existing = rulesById(root, schema.elements, schema.id)

  let added = 0
  let updated = 0
  let removed = 0
  for (const id of new Set(removeIds)) {
    if (wanted.has(id)) throw new Error(`Rule ${id} is both supplied and marked for removal.`)
    const prior = existing.get(id)
    if (prior && xmlChildren(prior, schema.id).length) {
      prior.remove()
      removed++
    }
  }
  for (const [id, rule] of wanted) {
    const prior = existing.get(id)
    if (prior && JSON.stringify(elementValue(prior)) === JSON.stringify(elementValue(rule))) continue
    const replacement = importRule(rule, root, source.namespaceURI)
    if (prior) {
      prior.replaceWith(replacement)
      updated++
    } else {
      root.append(replacement)
      added++
    }
  }
  return added || updated || removed ? { root, added, updated, removed } : undefined
}

function rulesById(root: Element, elements: readonly string[], idName: string): Map<string, Element> {
  const result = new Map<string, Element>()
  for (const name of elements) {
    for (const rule of xmlChildren(root, name)) {
      const id = xmlChildren(rule, idName)[0]?.textContent
      const key = id || `\u0000${JSON.stringify(elementValue(rule))}`
      if (id && result.has(id)) throw new Error(`Rule ID ${id} appears more than once.`)
      result.set(key, rule)
    }
  }
  return result
}

function importRule(element: Element, destination: Element, namespace: string | null): Element {
  const document = destination.ownerDocument
  if (element.namespaceURI === destination.namespaceURI || element.namespaceURI !== namespace) return document.importNode(element, true)
  const name = destination.prefix ? `${destination.prefix}:${element.localName}` : element.localName
  const output = document.createElementNS(destination.namespaceURI, name)
  for (const attribute of element.attributes) {
    if (attribute.namespaceURI !== 'http://www.w3.org/2000/xmlns/') output.setAttributeNS(attribute.namespaceURI, attribute.name, attribute.value)
  }
  for (const node of element.childNodes) {
    output.append(node instanceof Element ? importRule(node, destination, namespace) : document.importNode(node, true))
  }
  return output
}

function elementValue(element: Element): unknown {
  const namespace = element.namespaceURI === 'http://s3.amazonaws.com/doc/2006-03-01/' ? null : element.namespaceURI
  const attributes = [...element.attributes].filter(attribute => attribute.namespaceURI !== 'http://www.w3.org/2000/xmlns/')
    .map(attribute => [attribute.namespaceURI, attribute.localName, attribute.value]).sort()
  const children = [...element.children].map(elementValue)
  return [namespace, element.localName, attributes, children.length ? children : element.textContent]
}

function orderedJson(value: JsonValue): JsonValue {
  if (Array.isArray(value)) return value.map(orderedJson)
  if (!value || typeof value !== 'object') return value
  return Object.fromEntries(Object.keys(value).sort().map(key => [key, orderedJson(value[key]!)]))
}

function equivalentConfiguration(section: CampaignConfiguration, left: string, right: string): boolean {
  if (section === 'policy') return JSON.stringify(orderedJson(readPolicy(left))) === JSON.stringify(orderedJson(readPolicy(right)))
  const before = parseConfiguration(section, left).xml!.documentElement
  const after = parseConfiguration(section, right).xml!.documentElement
  if (section === 'publicAccessBlock') return publicBlockKey(before) === publicBlockKey(after)
  return JSON.stringify(elementValue(before)) === JSON.stringify(elementValue(after))
}

function publicBlockKey(root: Element): string {
  return ['BlockPublicAcls', 'IgnorePublicAcls', 'BlockPublicPolicy', 'RestrictPublicBuckets']
    .map(flag => xmlChildren(root, flag)[0]?.textContent === 'true' ? '1' : '0').join('')
}
