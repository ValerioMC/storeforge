import { desktop } from '../../../common/tauri'
import type { QueryPage } from '../model/iam'
import type { RgwAccount, RgwChange, RgwChangeResult, RgwKind, RgwRow, RgwUsage, RgwUser, RgwUserIdentity } from '../model/rgw'

export function listRgwIdentities(connection: string, kind: RgwKind, managed: boolean, marker?: string): Promise<QueryPage<RgwRow>> {
  return desktop('read_rgw_identity', { connection, request: { action: 'list', kind, managed, marker } })
}

export function readRgwAccount(connection: string, id: string): Promise<RgwAccount> {
  return desktop('read_rgw_identity', { connection, request: { action: 'account', id } })
}

export function readRgwAccountUsage(connection: string, id: string): Promise<RgwUsage> {
  return desktop('read_rgw_identity', { connection, request: { action: 'accountUsage', id } })
}

export function readRgwUser(connection: string, identity: RgwUserIdentity): Promise<RgwUser> {
  return desktop('read_rgw_identity', { connection, request: { action: 'user', identity } })
}

export function readDelegatedRgwUser(connection: string): Promise<RgwUser> {
  return desktop('read_rgw_identity', { connection, request: { action: 'delegatedKeys' } })
}

export function changeRgwIdentity(connection: string, request: RgwChange): Promise<RgwChangeResult> {
  return desktop('change_rgw_identity', { connection, request })
}
