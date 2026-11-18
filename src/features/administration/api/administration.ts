import { desktop } from '../../../common/tauri'
import type { AdminRequest } from '../model/operations'

export function administer(connection: string, request: AdminRequest): Promise<string> {
  return desktop<string>('administer', { connection, request })
}
