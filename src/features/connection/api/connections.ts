import { desktop, isTauri } from '../../../common/tauri'
import type { Connection, ConnectionCheck, ConnectionDraft } from '../model/connection'

export const connectionsApi = {
  list: () => isTauri() ? desktop<Connection[]>('list_connections') : Promise.resolve([]),
  save: (draft: ConnectionDraft) => desktop<Connection>('save_connection', { draft }),
  test: (draft: ConnectionDraft) => desktop<ConnectionCheck>('test_connection', { draft }),
  remove: (id: string) => desktop<void>('remove_connection', { id }),
}
