import { desktop } from '../../../common/tauri'
import type { CollaboratorReview, Collaborators, KeyPermission, KeyReceipt, MembershipReceipt, PortalKey, PortalKeys, PublicLink, SpaceAccessSummary, SpaceShare } from '../model/sharing'
import type { GrantRole, PortalScope, ProjectRole, SpaceScope } from '../model/spaces'
import type { ClientExport } from '../model/client-export'

type Read =
  | { kind: 'collaborators' | 'access' | 'links' | 'keys' }
  | { kind: 'collaborator'; user_id: number }

type Change =
  | { kind: 'audience'; share_scope: 'account' | 'restricted'; account_member_role: GrantRole | null }
  | { kind: 'grant' | 'changeRole'; user_id: number; role: GrantRole }
  | { kind: 'revoke'; user_id: number }
  | { kind: 'createLink'; object_key: string; label: string | null; expires_at: string | null }
  | { kind: 'revokeLink'; link_id: number }
  | { kind: 'personalKey' }
  | { kind: 'externalKey'; storage_space_id: string; external_email: string; permission: KeyPermission }
  | { kind: 'keyStatus'; key_id: string; active: boolean }
  | { kind: 'deleteKey'; key_id: string }
  | { kind: 'addMember'; target_name: string; target_email: string; portal_role: ProjectRole; reason: string | null }
  | { kind: 'removeMember'; target_name: string | null; target_email: string; reason: string | null }
  | { kind: 'memberRole'; target_user_id: number; portal_role: ProjectRole; reason: string | null }

function read<T>(scope: PortalScope | SpaceScope, view: Read): Promise<T> {
  return desktop<T>('read_portal', { ...scope, operation: { kind: 'sharing', view } })
}

function change<T>(scope: PortalScope | SpaceScope, action: Change): Promise<T> {
  return desktop<T>('change_portal', { ...scope, operation: { kind: 'sharing', action } })
}

export const sharingApi = {
  collaborators: (scope: PortalScope) => read<Collaborators>(scope, { kind: 'collaborators' }),
  collaborator: (scope: PortalScope, user_id: number) => read<CollaboratorReview>(scope, { kind: 'collaborator', user_id }),
  access: (scope: SpaceScope) => read<SpaceAccessSummary>(scope, { kind: 'access' }),
  setAudience: (scope: SpaceScope, share_scope: 'account' | 'restricted', account_member_role: GrantRole | null) =>
    change<void>(scope, { kind: 'audience', share_scope, account_member_role }),
  grant: (scope: SpaceScope, user_id: number, role: GrantRole) => change<SpaceShare>(scope, { kind: 'grant', user_id, role }),
  role: (scope: SpaceScope, user_id: number, role: GrantRole) => change<SpaceShare>(scope, { kind: 'changeRole', user_id, role }),
  revoke: (scope: SpaceScope, user_id: number) => change<SpaceShare[]>(scope, { kind: 'revoke', user_id }),
  links: (scope: SpaceScope) => read<PublicLink[]>(scope, { kind: 'links' }),
  createLink: (scope: SpaceScope, object_key: string, label: string | null, expires_at: string | null) =>
    change<PublicLink>(scope, { kind: 'createLink', object_key, label, expires_at }),
  revokeLink: (scope: SpaceScope, link_id: number) => change<PublicLink[]>(scope, { kind: 'revokeLink', link_id }),
  keys: (scope: PortalScope) => read<PortalKeys>(scope, { kind: 'keys' }),
  personalKey: (scope: PortalScope) => change<KeyReceipt>(scope, { kind: 'personalKey' }),
  externalKey: (scope: PortalScope, storage_space_id: string, external_email: string, permission: KeyPermission) =>
    change<KeyReceipt>(scope, { kind: 'externalKey', storage_space_id, external_email, permission }),
  keyStatus: (scope: PortalScope, key_id: string, active: boolean) => change<PortalKey>(scope, { kind: 'keyStatus', key_id, active }),
  deleteKey: (scope: PortalScope, key_id: string) => change<void>(scope, { kind: 'deleteKey', key_id }),
  addMember: (scope: PortalScope, target_name: string, target_email: string, portal_role: ProjectRole, reason: string | null) =>
    change<MembershipReceipt>(scope, { kind: 'addMember', target_name, target_email, portal_role, reason }),
  removeMember: (scope: PortalScope, target_name: string | null, target_email: string, reason: string | null) =>
    change<MembershipReceipt>(scope, { kind: 'removeMember', target_name, target_email, reason }),
  memberRole: (scope: PortalScope, target_user_id: number, portal_role: ProjectRole, reason: string | null) =>
    change<MembershipReceipt>(scope, { kind: 'memberRole', target_user_id, portal_role, reason }),
  exportClient: (format: ClientExport, document: string) => desktop<boolean>('export_portal_client', { format, document }),
}
