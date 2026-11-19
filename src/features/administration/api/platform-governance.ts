import { desktop } from '../../../common/tauri'
import type {
  AccessFilters, AccessReviewRow, GovernancePage, GroupDraft, ListingQuery, PlatformConnection, PlatformGroup, PrivateConnection,
} from '../model/platform-governance'

export const governanceApi = {
  access: (id: string, page: number, filters: AccessFilters) =>
    desktop<GovernancePage<AccessReviewRow>>('list_platform_access', { id, page, filters }),
  exportAccess: (id: string, filters: AccessFilters) =>
    desktop<boolean>('export_platform_access', { id, filters }),
  groups: (id: string, query: ListingQuery) =>
    desktop<GovernancePage<PlatformGroup>>('list_platform_groups', { id, query }),
  connections: (id: string, query: ListingQuery) =>
    desktop<GovernancePage<PlatformConnection>>('list_platform_connections', { id, query }),
  privateConnections: (id: string) =>
    desktop<PrivateConnection[]>('list_private_platform_connections', { id }),
  saveGroup: (id: string, groupId: number | null, draft: GroupDraft) =>
    desktop<PlatformGroup>('save_platform_group', { id, groupId, draft }),
  renameConnection: (id: string, connectionId: number, name: string) =>
    desktop<PlatformConnection>('rename_platform_connection', { id, connectionId, name }),
}
