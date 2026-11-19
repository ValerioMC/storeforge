import { desktop } from '../../../common/tauri'
import type {
  Candidate, FileDetail, FileListing, FileVersions, Page, PortalActivity, PortalAlert, PortalHealth, PortalProject,
  PortalScope, PortalSpace, PortalState, PortalTrends, PortalUsage, SpaceIcon, SpaceOperationResult, SpaceProvisioning,
  SpaceScope, SpaceSettings, SpaceSettingsDraft, SpaceStatistics, TrashListing,
} from '../model/spaces'

type Read =
  | { kind: 'projects' | 'state' | 'usage' | 'trends' | 'health' | 'alerts' | 'activity' | 'spaces' | 'space' | 'statistics' | 'spaceSettings' }
  | { kind: 'files'; prefix: string; cursor?: string }
  | { kind: 'detail'; key: string }
  | ({ kind: 'trash' } & VersionMarker)
  | ({ kind: 'versions'; key: string } & VersionMarker)
  | { kind: 'candidates'; search: string; page: number }

interface VersionMarker { key_marker?: string; version_id_marker?: string }
type CreateDraft = SpaceProvisioning & { name: string; naming_mode: 'generic_uuid' | 'named_bucket' }
type ImportDraft = SpaceProvisioning & { bucket_name: string }
type EditDraft = { name?: string; description: string }

type Change =
  | { kind: 'create'; draft: CreateDraft }
  | { kind: 'import'; draft: ImportDraft }
  | { kind: 'edit'; draft: EditDraft }
  | { kind: 'spaceSettings'; draft: SpaceSettingsDraft }
  | { kind: 'icon'; draft: { source: 'preset'; preset: SpaceIcon | null } }
  | { kind: 'archive'; archived: boolean }
  | { kind: 'takeOwnership' }
  | { kind: 'delete' | 'cleanup'; confirmation: string }
  | { kind: 'folder' | 'restorePrefix'; prefix: string }
  | { kind: 'deleteFile'; key: string }
  | { kind: 'restoreVersion'; key: string; version_id: string }

function read<T>(scope: { id: string; project?: number; space?: string }, operation: Read): Promise<T> {
  return desktop<T>('read_portal', { ...scope, operation })
}

function change<T>(scope: PortalScope | SpaceScope, operation: Change): Promise<T> {
  return desktop<T>('change_portal', { ...scope, operation })
}

export const portalApi = {
  projects: (id: string) => read<PortalProject[]>({ id }, { kind: 'projects' }),
  state: (scope: PortalScope) => read<PortalState>(scope, { kind: 'state' }),
  usage: (scope: PortalScope) => read<PortalUsage>(scope, { kind: 'usage' }),
  trends: (scope: PortalScope) => read<PortalTrends>(scope, { kind: 'trends' }),
  health: (scope: PortalScope) => read<PortalHealth>(scope, { kind: 'health' }),
  alerts: (scope: PortalScope) => read<PortalAlert[]>(scope, { kind: 'alerts' }),
  activity: (scope: PortalScope) => read<PortalActivity[]>(scope, { kind: 'activity' }),
  spaces: (scope: PortalScope) => read<PortalSpace[]>(scope, { kind: 'spaces' }),
  space: (scope: SpaceScope) => read<PortalSpace>(scope, { kind: 'space' }),
  statistics: (scope: SpaceScope) => read<SpaceStatistics>(scope, { kind: 'statistics' }),
  settings: (scope: SpaceScope) => read<SpaceSettings>(scope, { kind: 'spaceSettings' }),
  files: (scope: SpaceScope, prefix: string, cursor?: string) => read<FileListing>(scope, { kind: 'files', prefix, cursor }),
  detail: (scope: SpaceScope, key: string) => read<FileDetail>(scope, { kind: 'detail', key }),
  trash: (scope: SpaceScope, marker: VersionMarker = {}) => read<TrashListing>(scope, { kind: 'trash', ...marker }),
  versions: (scope: SpaceScope, key: string, marker: VersionMarker = {}) => read<FileVersions>(scope, { kind: 'versions', key, ...marker }),
  candidates: (scope: PortalScope | SpaceScope, search: string, page = 1) => read<Page<Candidate>>(scope, { kind: 'candidates', search, page }),
  create: (scope: PortalScope, draft: CreateDraft) => change<PortalSpace>(scope, { kind: 'create', draft }),
  import: (scope: PortalScope, draft: ImportDraft) => change<PortalSpace>(scope, { kind: 'import', draft }),
  edit: (scope: SpaceScope, draft: EditDraft) => change<PortalSpace>(scope, { kind: 'edit', draft }),
  setIcon: (scope: SpaceScope, preset: SpaceIcon | null) => change<void>(scope, { kind: 'icon', draft: { source: 'preset', preset: preset ?? 'bucket' } }),
  archive: (scope: SpaceScope, archived: boolean) => change<PortalSpace>(scope, { kind: 'archive', archived }),
  takeOwnership: (scope: SpaceScope) => change<PortalSpace>(scope, { kind: 'takeOwnership' }),
  delete: (scope: SpaceScope, confirmation: string) => change<void>(scope, { kind: 'delete', confirmation }),
  saveSettings: (scope: SpaceScope, draft: SpaceSettingsDraft) => change<SpaceSettings>(scope, { kind: 'spaceSettings', draft }),
  folder: (scope: SpaceScope, prefix: string) => change<void>(scope, { kind: 'folder', prefix }),
  deleteFile: (scope: SpaceScope, key: string) => change<void>(scope, { kind: 'deleteFile', key }),
  restore: (scope: SpaceScope, key: string, version_id: string) => change<{ message: string }>(scope, { kind: 'restoreVersion', key, version_id }),
  restorePrefix: (scope: SpaceScope, prefix: string) => change<SpaceOperationResult>(scope, { kind: 'restorePrefix', prefix }),
  cleanup: (scope: SpaceScope, confirmation: string) => change<SpaceOperationResult>(scope, { kind: 'cleanup', confirmation }),
  upload: (scope: SpaceScope, key: string, source: string) => desktop<void>('portal_upload', { ...scope, key, source }),
  preview: (scope: SpaceScope, key: string) => desktop<ArrayBuffer>('portal_preview', { ...scope, key }),
  download: (scope: SpaceScope, key: string, version: string | undefined, destination: string) =>
    desktop<void>('portal_download', { ...scope, key, version, destination }),
}
