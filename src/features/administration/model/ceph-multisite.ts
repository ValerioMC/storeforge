import type { Connection } from '../../connection/model/connection'

export type Availability = 'available' | 'denied' | 'missing' | 'unsupported' | 'unavailable' | 'stale' | 'notApplicable'
export interface Observation<T> { availability: Availability; status: number | null; data: T | null }
export interface ProfileScope { connection: string; revision: string | null }
export interface LocalZone { id: string; name: string; realmId: string | null }
export interface Realm { id: string; name: string; currentPeriod: string | null }
export interface Zone {
  id: string; name: string; groupId: string; groupName: string; readOnly: boolean | null
  syncFromAll: boolean | null; syncFrom: string[]; policyConfigured: boolean
}
export interface Period {
  id: string; realmId: string; epoch: string | null; realmEpoch: string | null; masterZone: string | null; zones: Zone[]
}
export interface RegisteredZone { scope: ProfileScope; name: string; local: Observation<LocalZone> }
export interface Topology {
  anchor: ProfileScope; collectedAt: string; realm: Observation<Realm>; period: Observation<Period>; registrations: RegisteredZone[]
}
export interface ZoneBinding { scope: ProfileScope; zone: string; realm: string; period: string; epoch: string | null }
export type FlowFamily = 'metadata' | 'data'
export interface SyncShard { id: number; phase: string; marker: string | null; timestamp: string | null }
export interface SyncStatus {
  phase: string; expectedShards: number | null; generation: string | null; period: string | null; shards: SyncShard[]
}
export interface FlowScope { destination: ZoneBinding; family: FlowFamily; sourceZone: string }
export interface FlowDirection { family: FlowFamily; source: Zone; destination: Zone; key: string }
export interface ZoneMapping { zone: Zone; state: 'mapped' | 'ambiguous' | 'missing'; endpoints: RegisteredZone[] }
export interface BucketSelector { name: string; tenant: string; instance: string | null }
export interface BucketIdentity extends BucketSelector { instance: string }
export interface Generation { generation: string; shards: number }
export interface BilogInfo { latest: string | null; oldest: string | null; generations: Generation[] }
export interface ReplicationScope {
  source: ZoneBinding; destination: ZoneBinding; sourceBucket: BucketSelector; destinationBucket: BucketSelector
}
export interface Replication {
  source: Observation<BucketIdentity>; destination: Observation<BucketIdentity>; sync: Observation<SyncStatus>; bilog: Observation<BilogInfo>
}
export type Journal =
  | { kind: 'metadata'; period: string; shard: number }
  | { kind: 'data'; shard: number }
  | { kind: 'bilog'; bucket: BucketSelector; generation: string; shard: number | null }
export interface JournalScope { target: ZoneBinding; journal: Journal; marker: string | null; limit: number }
export interface JournalEntry {
  id: string | null; key: string | null; timestamp: string | null; category: string | null
  operation: string | null; generation: string | null; instance: string | null
}
export interface JournalPage {
  entries: JournalEntry[]; nextMarker: string | null; nextGeneration: Generation | null; lastUpdate: string | null; bucket: BucketIdentity | null
}
export interface LoadedJournal {
  scope: JournalScope; entries: JournalEntry[]; nextMarker: string | null; nextGeneration: Generation | null
  lastUpdate: string | null; markers: string[]
}
export interface JournalSelection {
  family: Journal['kind']; gateway: string; period: string; shard: string; bucket: BucketSelector; generation: string
}

export const availabilityLabels: Record<Availability, string> = {
  available: 'Available', denied: 'Denied', missing: 'Missing', unsupported: 'Unsupported',
  unavailable: 'Unavailable', stale: 'Stale binding', notApplicable: 'Not applicable',
}

export function profileScope(profile: Pick<Connection, 'id' | 'revision'>): ProfileScope {
  return { connection: profile.id, revision: profile.revision ?? null }
}

export function zoneMappings(topology: Topology): ZoneMapping[] {
  const period = topology.period.data
  if (!period) return []
  return period.zones.map(zone => {
    const endpoints = topology.registrations.filter(({ local }) =>
      local.availability === 'available' && local.data?.id === zone.id && local.data.realmId === period.realmId)
    const state = endpoints.length > 1 ? 'ambiguous' : endpoints.length ? 'mapped' : 'missing'
    return { zone, endpoints, state }
  })
}

export function bindZone(period: Period, endpoint: RegisteredZone): ZoneBinding | undefined {
  const zone = endpoint.local.data
  if (endpoint.local.availability !== 'available' || zone?.realmId !== period.realmId || !period.zones.some(item => item.id === zone.id)) return
  return { scope: endpoint.scope, realm: period.realmId, zone: zone.id, period: period.id, epoch: period.epoch }
}

export function zoneEndpoints(period: Period, registrations: RegisteredZone[]): { label: string; value: string; binding: ZoneBinding }[] {
  return registrations.flatMap(endpoint => {
    const binding = bindZone(period, endpoint)
    return binding ? [{ label: `${endpoint.name} · ${endpoint.local.data?.name}`, value: endpoint.scope.connection, binding }] : []
  })
}

export function zoneFlows(period: Period, destination: Zone): FlowDirection[] {
  return period.zones.filter(source => source.id !== destination.id).flatMap(source => {
    const flows: FlowDirection[] = []
    if (period.masterZone === source.id) flows.push({ family: 'metadata', source, destination, key: `metadata:${source.id}:${destination.id}` })
    if (destination.syncFromAll !== false || destination.policyConfigured || destination.syncFrom.some(zone => zone === source.id || zone === source.name)) {
      flows.push({ family: 'data', source, destination, key: `data:${source.id}:${destination.id}` })
    }
    return flows
  })
}

export function validGeneration(value: string): boolean {
  return /^\d{1,20}$/.test(value) && BigInt(value) <= 18446744073709551615n
}

export function journalRequest(selection: JournalSelection, binding: ZoneBinding): JournalScope | undefined {
  const shard = /^\d+$/.test(selection.shard) && Number(selection.shard) <= 4294967295 ? Number(selection.shard) : null
  let journal: Journal
  if (selection.family === 'bilog') {
    if (!selection.bucket.name || !validGeneration(selection.generation) || (selection.shard && shard === null)) return
    journal = { kind: 'bilog', bucket: { ...selection.bucket }, generation: selection.generation, shard }
  } else {
    if (shard === null || (selection.family === 'metadata' && !selection.period)) return
    journal = selection.family === 'metadata' ? { kind: 'metadata', period: selection.period, shard } : { kind: 'data', shard }
  }
  return { target: binding, journal, marker: null, limit: 100 }
}

export function appendJournalPage(scope: JournalScope, page: JournalPage, previous?: LoadedJournal): LoadedJournal {
  const markers = previous?.markers ?? []
  if (page.nextMarker && (page.nextMarker === scope.marker || markers.includes(page.nextMarker))) {
    throw new Error('The gateway returned a marker that was already used.')
  }
  const journal = scope.journal.kind === 'bilog' && page.bucket ? { ...scope.journal, bucket: page.bucket } : scope.journal
  return {
    scope: { ...scope, journal, marker: null }, entries: [...previous?.entries ?? [], ...page.entries],
    nextMarker: page.nextMarker, nextGeneration: page.nextGeneration, lastUpdate: page.lastUpdate ?? previous?.lastUpdate ?? null,
    markers: page.nextMarker ? [...markers, page.nextMarker] : markers,
  }
}

export function journalNextPage(view: LoadedJournal): JournalScope | undefined {
  if (view.nextMarker) return { ...view.scope, marker: view.nextMarker }
}

export function journalNextGeneration(view: LoadedJournal): JournalScope | undefined {
  const { journal } = view.scope
  const next = view.nextGeneration
  if (journal.kind !== 'bilog' || !next || view.nextMarker || next.generation === journal.generation ||
      (journal.shard !== null && journal.shard >= next.shards)) return
  return { ...view.scope, journal: { ...journal, generation: next.generation }, marker: null }
}

export function filterJournal(entries: JournalEntry[], query: string): JournalEntry[] {
  const search = query.trim().toLowerCase()
  return search ? entries.filter(entry => Object.values(entry).some(value => value?.toLowerCase().includes(search))) : entries
}

export function journalCsv(entries: JournalEntry[]): string {
  const headers = ['id', 'key', 'timestamp', 'category', 'operation', 'generation', 'instance'] satisfies (keyof JournalEntry)[]
  return [headers.join(','), ...entries.map(entry => headers.map(header => csvCell(entry[header] ?? '')).join(','))].join('\r\n')
}

function csvCell(value: string): string {
  const safe = /^[\s]*[=+\-@]|^[\t\r\n]/.test(value) ? `'${value}` : value
  return `"${safe.replaceAll('"', '""')}"`
}
