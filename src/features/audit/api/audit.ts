import { desktop, isTauri } from '../../../common/tauri'
import type { AuditEvent } from '../model/audit'

export function listAudit(): Promise<AuditEvent[]> {
  return isTauri() ? desktop('list_audit') : Promise.resolve([])
}
