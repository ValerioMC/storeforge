import { describe, expect, it } from 'vitest'
import {
  appendJournalPage, filterJournal, journalCsv, journalNextGeneration, journalNextPage, journalRequest,
  validGeneration, zoneEndpoints, zoneFlows, zoneMappings, type JournalEntry, type JournalPage,
  type JournalScope, type Observation, type Period, type RegisteredZone, type Topology, type Zone,
} from '../src/features/administration/model/ceph-multisite'

function available<T>(data: T): Observation<T> {
  return { availability: 'available', status: 200, data }
}

function zone(id: string, name: string, syncFrom: string[]): Zone {
  return { id, name, groupId: 'group', groupName: 'Storage', readOnly: null, syncFromAll: false, syncFrom, policyConfigured: false }
}

function endpoint(id: string, localZone: string, realmId = 'realm'): RegisteredZone {
  return { scope: { connection: id, revision: 'revision' }, name: id, local: available({ id: localZone, name: localZone, realmId }) }
}

const primary = zone('primary', 'Primary', [])
const replica = zone('replica', 'Replica', ['Primary'])
const archive = zone('archive', 'Archive', [])
const period: Period = { id: 'period', realmId: 'realm', epoch: '2', realmEpoch: '4', masterZone: 'primary', zones: [primary, replica, archive] }
const registered = [endpoint('p', 'primary'), endpoint('r1', 'replica'), endpoint('r2', 'replica'), endpoint('outside', 'archive', 'other-realm'),
  { ...endpoint('denied', 'archive'), local: { availability: 'denied', status: 403, data: null } as const }]
const topology: Topology = { anchor: registered[0]!.scope, collectedAt: '2026-11-19T10:00:00Z',
  realm: available({ id: 'realm', name: 'Storage', currentPeriod: 'period' }), period: available(period), registrations: registered }
const binding = zoneEndpoints(period, registered)[0]!.binding
const journal: JournalScope = { target: binding, journal: { kind: 'bilog', bucket: { tenant: 'research', name: 'archive', instance: null },
  generation: '18446744073709551614', shard: 1 }, marker: null, limit: 100 }

function row(id: string, key: string): JournalEntry {
  return { id, key, timestamp: null, category: 'object', operation: 'put', generation: journal.journal.kind === 'bilog' ? journal.journal.generation : null, instance: null }
}

describe('Ceph multisite', () => {
  it('matches registered gateways by local zone and realm and preserves ambiguity', () => {
    expect(zoneMappings(topology).map(({ state, endpoints }) => [state, endpoints.map(item => item.name)]))
      .toEqual([['mapped', ['p']], ['ambiguous', ['r1', 'r2']], ['missing', []]])
    expect(zoneEndpoints(period, registered).map(endpoint => endpoint.value)).toEqual(['p', 'r1', 'r2'])
    expect(zoneFlows(period, replica).map(flow => flow.family)).toEqual(['metadata', 'data'])
    expect(zoneFlows(period, archive).map(flow => flow.family)).toEqual(['metadata'])
  })

  it('paginates the same tenant, instance, and generation without moving generations implicitly', () => {
    const bucket = { tenant: 'research', name: 'archive', instance: 'bucket-id' }
    const nextGeneration = { generation: '18446744073709551615', shards: 3 }
    const page: JournalPage = { entries: [row('1', 'notes.txt')], nextMarker: 'after-1', nextGeneration, lastUpdate: null, bucket }
    const first = appendJournalPage(journal, page)
    expect(journalNextGeneration(first)).toBeUndefined()
    const next = journalNextPage(first)!
    expect(next).toEqual({ ...journal, journal: { ...journal.journal, bucket }, marker: 'after-1' })
    expect(() => appendJournalPage(next, page, first)).toThrow('marker')
    const second = appendJournalPage(next, { ...page, entries: [row('2', 'photo.png')], nextMarker: null }, first)
    expect(second.entries.map(entry => entry.id)).toEqual(['1', '2'])
    expect(journalNextGeneration(second)).toEqual({ ...journal, journal: { ...journal.journal, bucket, generation: nextGeneration.generation } })
  })

  it('reports filtered loaded rows and safely preserves literal identifiers in CSV', () => {
    const entries = [row('1', 'notes.txt'), row('2', '=HYPERLINK("malicious.example")'), row('3', 'NOTES.md')]
    const filtered = filterJournal(entries, 'notes')
    expect(filtered.map(entry => entry.id)).toEqual(['1', '3'])
    expect(journalCsv(filtered)).not.toContain('malicious.example')
    expect(journalCsv(entries)).toContain('"\'=HYPERLINK(""malicious.example"")"')
    expect(journalCsv(entries)).toContain('18446744073709551614')
  })

  it('accepts only an explicit valid generation before requesting a bucket journal', () => {
    const input = { family: 'bilog' as const, gateway: 'p', period: '', shard: '',
      bucket: { name: 'archive', tenant: 'research', instance: null }, generation: '' }
    expect(journalRequest(input, binding)).toBeUndefined()
    expect(validGeneration('18446744073709551616')).toBe(false)
    expect(journalRequest({ ...input, generation: '18446744073709551615' }, binding)?.journal)
      .toEqual({ kind: 'bilog', bucket: input.bucket, generation: '18446744073709551615', shard: null })
  })
})
