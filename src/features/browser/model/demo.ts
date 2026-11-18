import type { Bucket } from '../../bucket/model/bucket'
import { endpointSettings, type Connection } from '../../connection/model/connection'
import type { ObjectDetails, ObjectRow, ObjectVersion } from './objects'

export const demoConnection: Connection = {
  id: 'demo', name: 'Asteria Research', provider: 'ceph',
  endpoint: 'https://rgw.asteria.example', region: 'eu-west-1', pathStyle: true, knownBuckets: [],
  settings: { ...endpointSettings(), coordinates: { latitude: 67.8, longitude: 20.2 } },
}

export const demoBuckets: Bucket[] = [
  'mission-archive', 'downlink-raw', 'observatory-data', 'processing-cache', 'public-products', 'telemetry-backups',
].map(name => ({ name, createdAt: '2025-06-03T09:00:00Z' }))

export const demoPrefix = 'telemetry/pass-018/'
export const demoKey = `${demoPrefix}pass-018-summary.csv`

const stamp = '2026-11-18T09:24:00Z'
const checksum = '7b9a4bc128a13bc724d88bea61a174d4'

const files: [string, number, string, string][] = [
  ['pass-018-summary.csv', 348198, stamp, 'STANDARD'],
  ['attitude-quaternions.parquet', 24611200, stamp, 'STANDARD'],
  ['downlink-manifest.json', 6874, stamp, 'STANDARD'],
  ['frame-index.ndjson', 1145080, '2026-11-18T09:21:00Z', 'STANDARD'],
  ['ground-track.geojson', 58621, '2026-11-18T09:19:00Z', 'STANDARD'],
  ['packet-loss-report.txt', 1428, '2026-11-18T09:18:00Z', 'STANDARD'],
  ['radio-calibration.bin', 1763200, '2026-11-18T09:13:00Z', 'STANDARD_IA'],
  ['sha256sums.txt', 1102, '2026-11-18T09:12:00Z', 'STANDARD'],
  ['station-metadata.yaml', 3482, '2026-11-18T09:12:00Z', 'STANDARD'],
]

const objects: ObjectRow[] = [
  ...['frames/', 'quality-control/', 'spectra/'].map(key => ({
    key: demoPrefix + key, folder: true, size: null, modified: null, storageClass: null, etag: null,
  })),
  ...files.map(([key, size, modified, storageClass]) => ({
    key: demoPrefix + key, folder: false, size, modified, storageClass, etag: checksum,
  })),
  { key: 'telemetry/pass-017/contacts.csv', folder: false, size: 2872, modified: stamp, storageClass: 'STANDARD', etag: checksum },
  { key: 'README.txt', folder: false, size: 2872, modified: stamp, storageClass: 'STANDARD', etag: checksum },
]

export function demoObjects(bucket: string, prefix: string, recursive: boolean): ObjectRow[] {
  if (bucket !== demoBuckets[0]?.name) return []
  const rows = new Map<string, ObjectRow>()
  for (const entry of objects.filter(row => row.key.startsWith(prefix) && row.key !== prefix)) {
    const relative = entry.key.slice(prefix.length)
    const separator = relative.indexOf('/')
    if (separator >= 0 && !recursive) {
      const key = prefix + relative.slice(0, separator + 1)
      rows.set(key, { key, folder: true, size: null, modified: null, storageClass: null, etag: null })
    } else if (!entry.folder || !recursive) {
      rows.set(entry.key, entry)
    }
  }
  return Array.from(rows.values())
}

export function demoDetails(key: string): ObjectDetails {
  const row = objects.find(object => object.key === key)
  return {
    contentType: key.endsWith('.csv') ? 'text/csv' : 'application/octet-stream', size: row?.size ?? 0,
    etag: row?.etag ?? checksum, modified: row?.modified ?? stamp, versionId: 'p018-atHj82kL9a',
    storageClass: row?.storageClass ?? 'STANDARD', retainUntil: null, retentionMode: null, legalHold: null,
    restore: null, cacheControl: null, contentDisposition: null, contentEncoding: null, contentLanguage: null,
    expires: null, encryption: 'AES256', metadata: { mission: 'asteria', station: 'kiruna', pass: '018' },
  }
}

export function demoVersions(prefix: string, exact: boolean): ObjectVersion[] {
  return objects.filter(row => !row.folder && (exact ? row.key === prefix : row.key.startsWith(prefix)))
    .flatMap(row => [
      { key: row.key, versionId: 'p018-atHj82kL9a', latest: true, deleteMarker: false, size: row.size, modified: row.modified },
      { key: row.key, versionId: 'p018-a2nd42m28', latest: false, deleteMarker: false, size: 340102, modified: '2026-11-17T17:10:00Z' },
    ])
}
