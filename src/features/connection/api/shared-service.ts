import { desktop } from '../../../common/tauri'
import type {
  SharedAction, SharedContext, SharedServiceDraft, SharedServiceProfile, SharedServiceSession, SharedServiceStatus, SharedWorkspace,
} from '../model/shared-service'

export const sharedServiceApi = {
  list: () => desktop<SharedServiceStatus[]>('list_shared_services'),
  save: (id: string | null, draft: SharedServiceDraft) => desktop<SharedServiceProfile>('save_shared_service', { id, draft }),
  connect: (id: string, token: string) => desktop<SharedServiceSession>('connect_shared_service', { id, token }),
  disconnect: (id: string, forget: boolean) => desktop<void>('disconnect_shared_service', { id, forget }),
  contexts: (id: string, workspace: SharedWorkspace) => desktop<SharedContext[]>('read_shared_contexts', { id, workspace }),
  open: (id: string, action: SharedAction) => desktop<void>('open_shared_action', { id, action }),
}
