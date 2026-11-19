import { executionContext, identityAvailable, type Connection } from '../../connection/model/connection'
import type { BucketQuota, Configuration } from './bucket'
import { extractRules, parseConfiguration, readConfiguration, type ConfigurationReading } from './workspace'
import { xmlChildren } from './xml-document'

export type InventorySource = 'storageOps' | 'cephAdmin'
export type ExportFormat = 'text' | 'csv' | 'json'

export interface InventoryBucket {
  name: string
  tenant: string
  instance: string | null
  owner: string | null
  createdAt: string | null
  bytes: number | null
  objects: number | null
  quota: BucketQuota | null
}

export interface BucketTarget {
  connection: string
  revision: string | null
  source: InventorySource
  tenant: string
  name: string
  instance: string | null
}

export interface WorkbenchRow extends InventoryBucket {
  key: string
  target: BucketTarget
  profile: Connection
  configurationAvailable: boolean
  quotaAvailable: boolean
}

type Cell = string | number | boolean | null
export type FeatureReader = (row: WorkbenchRow, section: Configuration) => ConfigurationReading | undefined
export type TagReader = (row: WorkbenchRow) => readonly string[]
export const unavailableWorkbenchConfiguration = {
  status: 'unsupported', label: 'Unsupported', message: 'S3 configuration is unavailable in this context.',
} as const satisfies ConfigurationReading

interface Column {
  id: string
  label: string
  section?: Configuration
  quota?: boolean
  value: (row: WorkbenchRow, reading: ConfigurationReading | undefined, tags: readonly string[]) => Cell
}

export const workbenchColumns = [
  { id: 'bucket', label: 'Bucket', value: row => row.name },
  { id: 'context', label: 'Context', value: row => row.profile.name },
  { id: 'identity', label: 'Identity', value: row => row.target.source === 'cephAdmin' ? 'Ceph Admin' : 'S3' },
  { id: 'endpoint', label: 'Endpoint', value: row => row.profile.endpoint || `AWS · ${row.profile.region}` },
  { id: 'tenant', label: 'Tenant', value: row => row.tenant },
  { id: 'account', label: 'Account', value: row => row.profile.settings?.account || null },
  { id: 'owner', label: 'RGW owner', value: row => row.owner },
  { id: 'instance', label: 'RGW instance', value: row => row.instance },
  { id: 'created', label: 'Created', value: row => row.createdAt },
  { id: 'bytes', label: 'Storage bytes', value: row => row.bytes },
  { id: 'objects', label: 'Objects', value: row => row.objects },
  { id: 'quota', quota: true, label: 'Quota enforcement', value: row => row.quota?.enabled ?? null },
  { id: 'quotaBytes', quota: true, label: 'Quota bytes', value: row => row.quota ? row.quota.maxBytes || 'Unlimited' : null },
  { id: 'quotaObjects', quota: true, label: 'Quota objects', value: row => row.quota ? row.quota.maxObjects || 'Unlimited' : null },
  { id: 'quotaUsed', quota: true, label: 'Storage quota used (%)', value: row => quotaPercent(row.bytes, row.quota?.maxBytes, row.quota?.enabled) },
  { id: 'quotaObjectsUsed', quota: true, label: 'Object quota used (%)', value: row => quotaPercent(row.objects, row.quota?.maxObjects, row.quota?.enabled) },
  { id: 'uiTags', label: 'UI tags', value: (_, __, tags) => tags.join(', ') },
  { id: 'nativeTags', label: 'S3 tags', section: 'tagging', value: (_, reading) => nativeTags(reading) },
  { id: 'versioning', label: 'Versioning', section: 'versioning', value: (_, reading) => featureLabel(reading) },
  { id: 'lifecycle', label: 'Lifecycle rules', section: 'lifecycle', value: (_, reading) => lifecycleState(reading) },
  { id: 'retention', label: 'Lifecycle expiration', section: 'lifecycle', value: (_, reading) => lifecycleActions(reading, 'Expiration', 'NoncurrentVersionExpiration') },
  { id: 'multipart', label: 'Multipart cleanup', section: 'lifecycle', value: (_, reading) => lifecycleActions(reading, 'AbortIncompleteMultipartUpload') },
  { id: 'objectLock', label: 'Object Lock', section: 'objectLock', value: (_, reading) => featureLabel(reading) },
  { id: 'defaultRetention', label: 'Default retention', section: 'objectLock', value: (_, reading) => retention(reading) },
  { id: 'encryption', label: 'Encryption algorithm', section: 'encryption', value: (_, reading) => xmlValues(reading, 'encryption', 'SSEAlgorithm') },
  { id: 'kmsKey', label: 'KMS key', section: 'encryption', value: (_, reading) => xmlValues(reading, 'encryption', 'KMSMasterKeyID') },
  { id: 'publicAccess', label: 'Block public access', section: 'publicAccessBlock', value: (_, reading) => featureLabel(reading) },
  { id: 'blockPublicAcls', label: 'Block public ACLs', section: 'publicAccessBlock', value: (_, reading) => xmlValues(reading, 'publicAccessBlock', 'BlockPublicAcls') },
  { id: 'ignorePublicAcls', label: 'Ignore public ACLs', section: 'publicAccessBlock', value: (_, reading) => xmlValues(reading, 'publicAccessBlock', 'IgnorePublicAcls') },
  { id: 'publicPolicy', label: 'Block public policies', section: 'publicAccessBlock', value: (_, reading) => xmlValues(reading, 'publicAccessBlock', 'BlockPublicPolicy') },
  { id: 'restrictPublicBuckets', label: 'Restrict public buckets', section: 'publicAccessBlock', value: (_, reading) => xmlValues(reading, 'publicAccessBlock', 'RestrictPublicBuckets') },
  { id: 'policy', label: 'Bucket policy', section: 'policy', value: (_, reading) => featureLabel(reading) },
  { id: 'policyPublic', label: 'Policy allows wildcard principal', section: 'policy', value: (_, reading) => publicPolicy(reading) },
  { id: 'publicAcl', label: 'ACL grants to public groups', section: 'acl', value: (_, reading) => publicAcl(reading) },
  { id: 'cors', label: 'CORS', section: 'cors', value: (_, reading) => featureLabel(reading) },
  { id: 'notification', label: 'Notification destinations', section: 'notification', value: (_, reading) => xmlValues(reading, 'notification', 'Topic', 'Queue', 'CloudFunction') },
  { id: 'notificationState', label: 'Notification configuration', section: 'notification', value: (_, reading) => featureLabel(reading) },
  { id: 'logging', label: 'Access logging', section: 'logging', value: (_, reading) => featureLabel(reading) },
  { id: 'replication', label: 'Replication', section: 'replication', value: (_, reading) => featureLabel(reading) },
] satisfies Column[] as Column[]

export const defaultColumns = ['bucket', 'context', 'tenant', 'bytes', 'objects', 'uiTags']
export const filtersOperators = [
  { value: 'contains', label: 'Contains' },
  { value: 'exact', label: 'Equals' },
  { value: 'gte', label: 'At least' },
  { value: 'lte', label: 'At most' },
  { value: 'unavailable', label: 'Unavailable' },
] as const
export type FilterOperator = typeof filtersOperators[number]['value']
export interface WorkbenchFilter { field: string; operator: FilterOperator; query: string }

export const workbenchAuditPresets: { id: string; label: string; filters: WorkbenchFilter[] }[] = [
  { id: 'retention', label: 'Lifecycle expiration enabled', filters: [{ field: 'retention', operator: 'exact', query: 'Enabled' }] },
  { id: 'object-lock', label: 'Object Lock enabled', filters: [{ field: 'objectLock', operator: 'exact', query: 'Enabled' }] },
  { id: 'multipart', label: 'Multipart cleanup enabled', filters: [{ field: 'multipart', operator: 'exact', query: 'Enabled' }] },
  { id: 'public-policy', label: 'Public exposure · wildcard policy grants', filters: [{ field: 'policyPublic', operator: 'exact', query: 'true' }] },
  { id: 'public-acl', label: 'Public exposure · public ACL grants', filters: [{ field: 'publicAcl', operator: 'exact', query: 'true' }] },
  { id: 'notifications', label: 'Notifications configured', filters: [{ field: 'notificationState', operator: 'exact', query: 'Configured' }] },
  { id: 'quota', label: 'Storage quota at least 80% used', filters: [{ field: 'quotaUsed', operator: 'gte', query: '80' }] },
  { id: 'encryption', label: 'Default encryption unconfigured', filters: [{ field: 'encryption', operator: 'exact', query: 'Not configured' }] },
  { id: 'kms', label: 'KMS encryption', filters: [{ field: 'encryption', operator: 'contains', query: 'kms' }] },
]

export function makeWorkbenchRow(profile: Connection, source: InventorySource, bucket: InventoryBucket): WorkbenchRow {
  const target: BucketTarget = { connection: profile.id, revision: profile.revision ?? null, source, tenant: bucket.tenant, name: bucket.name, instance: bucket.instance }
  const bound = bucket.tenant === (profile.settings?.tenant ?? '') && (source !== 'cephAdmin' || !!bucket.instance)
  return {
    ...bucket, key: JSON.stringify([profile.id, profile.endpointId ?? profile.id, profile.settings?.account ?? '', bucket.tenant, bucket.name, bucket.instance]), target, profile,
    configurationAvailable: bound && identityAvailable(profile, 's3'),
    quotaAvailable: bound && identityAvailable(profile, 'cephAdmin'),
  }
}

export function classificationKey(source: InventorySource, row: WorkbenchRow): string {
  const endpoint = executionContext(row.profile, source, source === 'cephAdmin' ? 'cephAdmin' : 's3')?.endpointId ?? row.profile.endpointId ?? row.profile.id
  return JSON.stringify([source, endpoint, row.tenant, row.name])
}

export function focusedRow(rows: readonly WorkbenchRow[], key?: string): WorkbenchRow | undefined {
  return rows.find(row => row.key === key)
}

export function adjacentRow(rows: readonly WorkbenchRow[], key: string, direction: -1 | 1): WorkbenchRow | undefined {
  const index = rows.findIndex(row => row.key === key)
  return index < 0 ? undefined : rows[index + direction]
}

export function rowCell(column: Column, row: WorkbenchRow, read: FeatureReader, tags: TagReader): Cell {
  return column.value(row, column.section ? read(row, column.section) : undefined, tags(row))
}

function quotaPercent(usage: number | null, maximum: string | undefined, enabled: boolean | undefined): number | null {
  const limit = Number(maximum)
  return enabled && usage !== null && limit > 0 ? Math.round(1000 * usage / limit) / 10 : null
}

function featureLabel(reading: ConfigurationReading | undefined): string | null {
  if (!reading) return null
  if (reading.status === 'error') return /^AccessDenied\b|\b403\b|\bForbidden\b/i.test(reading.message) ? 'Denied' : 'Read error'
  return reading.label
}

function lifecycleState(reading: ConfigurationReading | undefined): string | null {
  if (reading?.status !== 'configured') return featureLabel(reading)
  const statuses = new Set(extractRules('lifecycle', reading.document).map(rule => rule.status).filter(Boolean))
  return [...statuses].sort().join(', ') || 'Configured'
}

function nativeTags(reading: ConfigurationReading | undefined): string | null {
  if (reading?.status !== 'configured') return featureLabel(reading)
  return extractRules('tagging', reading.document).map(tag => `${tag.name}=${tag.detail}`).join(', ')
}

function lifecycleActions(reading: ConfigurationReading | undefined, ...names: string[]): string | null {
  if (reading?.status !== 'configured') return featureLabel(reading)
  const root = parseConfiguration('lifecycle', reading.document).xml!.documentElement
  const actions = xmlChildren(root, 'Rule').filter(rule =>
    xmlChildren(rule, 'Status')[0]?.textContent === 'Enabled' && names.some(name => xmlChildren(rule, name).length > 0))
  return actions.length ? 'Enabled' : 'Disabled'
}

function xmlValues(reading: ConfigurationReading | undefined, section: Configuration, ...names: string[]): string | null {
  if (reading?.status !== 'configured') return featureLabel(reading)
  const root = parseConfiguration(section, reading.document).xml!.documentElement
  const values = names.flatMap(name => Array.from(root.getElementsByTagNameNS(root.namespaceURI, name), node => node.textContent ?? ''))
  return [...new Set(values.filter(Boolean))].join(', ') || 'Not configured'
}

function retention(reading: ConfigurationReading | undefined): string | null {
  const mode = xmlValues(reading, 'objectLock', 'Mode')
  if (reading?.status !== 'configured' || mode === 'Not configured') return mode
  const days = xmlValues(reading, 'objectLock', 'Days')
  const years = xmlValues(reading, 'objectLock', 'Years')
  return [mode, days !== 'Not configured' ? `${days} days` : `${years} years`].join(' · ')
}

function isWildcardPrincipal(principal: unknown): boolean {
  if (principal === '*') return true
  if (!principal || typeof principal !== 'object' || Array.isArray(principal)) return false
  return Object.values(principal).some(value => value === '*' || Array.isArray(value) && value.includes('*'))
}

function publicPolicy(reading: ConfigurationReading | undefined): Cell {
  if (reading?.status !== 'configured') return featureLabel(reading)
  const { Statement } = parseConfiguration('policy', reading.document).policy!
  const statements = Array.isArray(Statement) ? Statement : [Statement]
  return statements.some(statement => statement && typeof statement === 'object' && !Array.isArray(statement) &&
    statement.Effect === 'Allow' && isWildcardPrincipal(statement.Principal))
}

function publicAcl(reading: ConfigurationReading | undefined): Cell {
  const groups = xmlValues(reading, 'acl', 'URI')
  return reading?.status === 'configured' ? !!groups?.split(', ').some(group =>
    group === 'http://acs.amazonaws.com/groups/global/AllUsers' || group === 'http://acs.amazonaws.com/groups/global/AuthenticatedUsers') : groups
}

export async function readWorkbenchFeature(row: WorkbenchRow, section: Configuration, read: () => Promise<string>): Promise<ConfigurationReading> {
  if (!row.configurationAvailable) return unavailableWorkbenchConfiguration
  return readConfiguration(read, row.name, section)
}

export function matchesCell(value: Cell, operator: FilterOperator, query: string): boolean {
  if (operator === 'unavailable') return value === null || value === 'Unsupported' || value === 'Denied' || value === 'Read error'
  const term = query.trim().toLowerCase()
  if (!term) return true
  if (value === null) return false
  if (operator === 'gte' || operator === 'lte') {
    if (typeof value === 'boolean' || value === '' || !Number.isFinite(Number(value)) || !Number.isFinite(Number(term))) return false
    return operator === 'gte' ? Number(value) >= Number(term) : Number(value) <= Number(term)
  }
  const text = String(value).toLowerCase()
  if (operator === 'exact') return text === term || text.split(', ').includes(term)
  return text.includes(term)
}

export function filterWorkbenchRows(
  rows: readonly WorkbenchRow[], search: string, mode: 'contains' | 'exact', context: string,
  filters: readonly WorkbenchFilter[], read: FeatureReader, tags: TagReader,
): WorkbenchRow[] {
  return rows.filter(row => (!context || row.profile.id === context) && matchesCell(row.name, mode, search) &&
    filters.every(filter => {
      const column = workbenchColumns.find(candidate => candidate.id === filter.field)
      return column ? matchesCell(rowCell(column, row, read, tags), filter.operator, filter.query) : true
    }))
}

function csvCell(value: Cell): string {
  const text = String(value ?? '')
  const safe = typeof value === 'string' && /^[\s\u0000-\u001f]*[=+\-@]/.test(value) ? `'${text}` : text
  return `"${safe.replaceAll('"', '""')}"`
}

export function exportWorkbenchRows(
  rows: readonly WorkbenchRow[], columns: readonly Column[], format: ExportFormat,
  read: FeatureReader, tags: TagReader,
): string {
  const headers = columns.map(column => column.label)
  const cells = rows.map(row => columns.map(column => rowCell(column, row, read, tags)))
  if (format === 'json') return JSON.stringify(cells.map(row => Object.fromEntries(headers.map((header, index) => [header, row[index]]))), null, 2)
  if (format === 'csv') return [headers, ...cells].map(row => row.map(csvCell).join(',')).join('\r\n')
  return [headers, ...cells].map(row => row.map(value => String(value ?? '').replace(/[\t\n\r]/g, ' ')).join('\t')).join('\n')
}
