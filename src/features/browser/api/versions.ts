import type { ObjectId, ObjectVersion, VersionCursor } from '../model/objects'
import { objectsApi } from './objects'

export interface VerifiedVersion {
  key: string
  latest: ObjectVersion | undefined
  removedVersionId?: string
}

export async function verifyVersionChange(
  connection: string,
  object: ObjectId,
  action: 'delete' | 'restoreVersion',
  previous: ObjectVersion | undefined,
  restoredVersionId?: string,
): Promise<VerifiedVersion> {
  let cursor: VersionCursor | null = null
  let latest: ObjectVersion | undefined
  const visited = new Set<string>()
  do {
    const page = await objectsApi.versions(connection, object.bucket, object.key, true, cursor)
    if (action === 'delete' && page.versions.some(item => item.versionId === object.versionId)) {
      throw new Error('The provider still lists the selected version. Refresh the version list to check its status.')
    }
    latest ??= page.versions.find(item => item.latest)
    cursor = page.cursor
    const marker = JSON.stringify(cursor)
    if (cursor && visited.has(marker)) throw new Error('The provider repeated the version cursor during verification.')
    if (cursor) visited.add(marker)
  } while (cursor && (action === 'delete' || !latest))

  if (action === 'restoreVersion') {
    const expected = restoredVersionId ? latest?.versionId === restoredVersionId : latest?.versionId !== previous?.versionId
    if (!latest || latest.deleteMarker || !expected) {
      throw new Error('The provider has not confirmed the restored object as current. Refresh the version list to check its status.')
    }
  }
  return { key: object.key, latest, removedVersionId: action === 'delete' ? object.versionId ?? undefined : undefined }
}
