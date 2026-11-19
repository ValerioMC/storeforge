import { desktop } from '../../../common/tauri'
import { demoDetails, demoObjects, demoVersions } from '../model/demo'
import type { Location, MultipartUpload, ObjectChange, ObjectDetails, ObjectId, ObjectPage, Preview, Tag, VersionCursor, VersionPage } from '../model/objects'

export const objectsApi = {
  list(location: Location, cursor: string | null, recursive: boolean): Promise<ObjectPage> {
    if (location.connection === 'demo') {
      return Promise.resolve({ rows: demoObjects(location.bucket, location.prefix, recursive), cursor: null })
    }
    return desktop('list_objects', { query: { location, cursor, recursive } })
  },
  details: (connection: string, object: ObjectId) => connection === 'demo' ? Promise.resolve(demoDetails(object.key)) : desktop<ObjectDetails>('object_details', { connection, object }),
  versions: (connection: string, bucket: string, prefix: string, exact: boolean, cursor: VersionCursor | null) =>
    connection === 'demo' ? Promise.resolve({ versions: demoVersions(prefix, exact), cursor: null }) : desktop<VersionPage>('list_versions', { connection, bucket, prefix, exact, cursor }),
  tags: (connection: string, object: ObjectId) => connection === 'demo' ? Promise.resolve([{ key: 'dataset', value: 'telemetry' }, { key: 'review', value: 'verified' }]) : desktop<Tag[]>('object_tags', { connection, object }),
  acl: (connection: string, object: ObjectId) => connection === 'demo' ? Promise.resolve(['FULL_CONTROL · asteria-operations']) : desktop<string[]>('object_acl', { connection, object }),
  preview: (connection: string, object: ObjectId) => connection === 'demo'
    ? Promise.resolve<Preview>({ contentType: 'text/csv', kind: 'text', oversized: false, data: btoa('timestamp,station,signal_dbm\n09:18:00,kiruna,-92\n09:19:00,kiruna,-87\n09:20:00,kiruna,-86'), truncated: false })
    : desktop<Preview>('preview_object', { connection, object }),
  share: (connection: string, object: ObjectId, minutes: number) => desktop<string>('share_object', { connection, object, minutes }),
  folder: (connection: string, bucket: string, key: string) => desktop<void>('create_folder', { connection, bucket, key }),
  change: (connection: string, object: ObjectId, change: ObjectChange) => desktop<string | null>('change_object', { connection, object, change }),
  multipart: (location: Location) => desktop<MultipartUpload[]>('list_multipart', { ...location }),
  abortMultipart: (connection: string, bucket: string, key: string, uploadId: string) => desktop<void>('abort_multipart', { connection, bucket, key, uploadId }),
}
