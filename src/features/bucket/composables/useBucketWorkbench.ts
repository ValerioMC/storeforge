import { computed, shallowReactive, watch, type Ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import type { Connection } from '../../connection/model/connection'
import { readMonitoring } from '../../monitoring/api/monitoring'
import type { BucketSample } from '../../monitoring/model/monitoring'
import { scopedBucketConfiguration, streamBucketInventory } from '../api/workbench'
import type { Configuration } from '../model/bucket'
import { makeWorkbenchRow, readWorkbenchFeature, type FeatureReader, type InventorySource, type WorkbenchRow } from '../model/workbench'

interface Snapshot {
  revision: string | null
  rows: readonly WorkbenchRow[]
  pending: boolean
  inventoryError: string
  usageError: string
  loadedAt: string | null
}

type Feature = NonNullable<ReturnType<FeatureReader>>

interface FeatureRequest {
  key: string
  row: WorkbenchRow
  section: Configuration
  revision: number
}

export function useBucketWorkbench(profiles: Readonly<Ref<readonly Connection[]>>, source: InventorySource) {
  const snapshots = shallowReactive(new Map<string, Snapshot>())
  const readings = shallowReactive(new Map<string, Feature>())
  const queue: FeatureRequest[] = []
  const pendingReads = new Map<string, number>()
  let inventoryRun = 0
  let featureRun = 0
  let workers = 0

  const rows = computed(() => profiles.value.flatMap(profile => snapshots.get(profile.id)?.rows ?? [])
    .sort((left, right) => left.name.localeCompare(right.name) || left.profile.name.localeCompare(right.profile.name) ||
      left.tenant.localeCompare(right.tenant) || (left.instance ?? '').localeCompare(right.instance ?? '')))
  const busy = computed(() => [...snapshots.values()].some(snapshot => snapshot.pending))

  watch(() => profiles.value.map(profile => [profile.id, profile.revision]), () => void refresh(false), { immediate: true })

  async function refresh(force = true): Promise<void> {
    const current = ++inventoryRun
    if (force) invalidate()
    const valid = new Set(profiles.value.map(profile => profile.id))
    for (const key of snapshots.keys()) if (!valid.has(key)) snapshots.delete(key)
    const targets = profiles.value.filter(profile => {
      const prior = snapshots.get(profile.id)
      return force || !prior || prior.pending || prior.revision !== (profile.revision ?? null)
    })
    const requests = targets.map(profile => {
      const revision = profile.revision ?? null
      const prior = snapshots.get(profile.id)
      const snapshot: Snapshot = { revision, rows: prior?.revision === revision ? prior.rows : [],
        pending: true, inventoryError: '', usageError: '', loadedAt: prior?.revision === revision ? prior.loadedAt : null }
      snapshots.set(profile.id, snapshot)
      return { profile, snapshot }
    })
    let position = 0
    async function next(): Promise<void> {
      while (current === inventoryRun) {
        const request = requests[position++]
        if (!request) return
        await load(request.profile, request.snapshot, current)
      }
    }
    await Promise.all(Array.from({ length: Math.min(4, requests.length) }, next))
  }

  async function load(profile: Connection, snapshot: Snapshot, current: number): Promise<void> {
    const received = new Map<string, WorkbenchRow>()
    const active = () => current === inventoryRun
    let usage = new Map<string, BucketSample>()
    let inventoryError = ''
    let usageError = ''
    function enriched(row: WorkbenchRow): WorkbenchRow {
      const sample = usage.get(row.name)
      return sample ? {
        ...row, bytes: sample.usage.value?.currentBytes ?? row.bytes, objects: sample.usage.value?.currentObjects ?? row.objects,
        quota: sample.quota.value ? {
          enabled: sample.quota.value.enabled, maxBytes: String(sample.quota.value.maxBytes ?? ''),
          maxObjects: String(sample.quota.value.maxObjects ?? ''),
        } : row.quota,
      } : row
    }
    function publish(complete: boolean): void {
      if (!active()) return
      const visible = complete && !inventoryError ? received : new Map([...snapshot.rows.map(row => [row.key, row] as const), ...received])
      snapshots.set(profile.id, {
        ...snapshot, rows: [...visible.values()].map(enriched), pending: !complete, inventoryError, usageError,
        loadedAt: complete && !inventoryError ? new Date().toISOString() : snapshot.loadedAt,
      })
    }

    const metadata = source === 'storageOps' && profile.id !== 'demo' ? readMonitoring(profile.id).then(view => {
      if (!active() || view.scope.revision !== snapshot.revision || view.scope.connectionId !== profile.id) return
      usage = new Map(view.usage?.buckets.map(sample => [sample.bucket, sample]))
      publish(false)
    }, failure => { usageError = errorMessage(failure) }) : Promise.resolve()
    try {
      await streamBucketInventory(profile, source, page => {
        if (!active()) return
        for (const bucket of page) {
          const row = makeWorkbenchRow(profile, source, bucket)
          received.set(row.key, row)
        }
        publish(false)
      })
    } catch (failure) {
      inventoryError = errorMessage(failure)
    }
    await metadata
    publish(true)
  }

  function readingKey(row: WorkbenchRow, section: Configuration): string {
    return JSON.stringify([row.key, row.target.revision, section])
  }

  function reading(row: WorkbenchRow, section: Configuration): Feature | undefined {
    return row.configurationAvailable ? readings.get(readingKey(row, section)) : { status: 'unsupported' }
  }

  async function readNext(): Promise<void> {
    workers++
    try {
      while (queue.length) {
        const request = queue.shift()!
        const { key, row, section, revision } = request
        if (pendingReads.get(key) !== revision) continue
        const result = await readWorkbenchFeature(row, section, () => scopedBucketConfiguration(row.target, section, { action: 'read' }))
        if (pendingReads.get(key) !== revision) continue
        pendingReads.delete(key)
        if (snapshots.get(row.profile.id)?.revision === row.target.revision) readings.set(key, result)
      }
    } finally {
      workers--
    }
  }

  function enrich(candidates: readonly WorkbenchRow[], sections: ReadonlySet<Configuration>): void {
    for (const row of candidates) {
      if (!row.configurationAvailable) continue
      for (const section of sections) {
        const key = readingKey(row, section)
        if (readings.has(key) || pendingReads.has(key)) continue
        const revision = ++featureRun
        pendingReads.set(key, revision)
        queue.push({ key, row, section, revision })
      }
    }
    for (let slots = Math.min(6 - workers, queue.length); slots > 0; slots--) void readNext()
  }

  function invalidate(row?: WorkbenchRow): void {
    if (!row) {
      readings.clear()
      pendingReads.clear()
      queue.splice(0)
      return
    }
    for (const key of readings.keys()) if (key.startsWith(JSON.stringify([row.key]).slice(0, -1))) readings.delete(key)
    for (const configuration of pendingReads.keys()) {
      if (configuration.startsWith(JSON.stringify([row.key]).slice(0, -1))) pendingReads.delete(configuration)
    }
  }

  return { snapshots, rows, busy, refresh, enrich, reading, invalidate }
}
