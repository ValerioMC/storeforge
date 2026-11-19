import type { Configuration } from '../../bucket/model/bucket'

export interface MigrationMapping {
  source: string
  target: string
}

export interface MigrationDraft {
  source: string
  target: string
  mappings: MigrationMapping[]
  mode: 'preCopy' | 'immediate'
  method: 'streamed' | 'storage'
  history: boolean
  settings: boolean
  parallelism: number
  protectDestination: boolean
  grantSourceRead: boolean
}

export type MigrationPhase = 'read' | 'active'

export interface MigrationReport {
  revision: string
  phase: MigrationPhase
  checkedAt: string
  expiresAt: string
  pairs: {
    mapping: MigrationMapping
    sample: string | null
    settingsCopied: Configuration[]
    settingsOmitted: Configuration[]
    issues: string[]
  }[]
}

export interface Migration {
  id: string
  revision: string
  draft: MigrationDraft
  updatedAt: string
  report: MigrationReport | null
  checking: MigrationPhase | null
  run: MigrationRun | null
  canRestore: boolean
  recoveryRequired: boolean
  recoveryError: string | null
}

export function migrationStatus(migration: Migration, now: number): string {
  if (migration.checking) return migration.checking === 'active' ? 'Running active checks' : 'Running read-only checks'
  if (migration.recoveryRequired) return 'Restore access'
  if (migration.run) return runLabels[migration.run.status]
  const report = migration.report
  if (!report || report.revision !== migration.revision) return 'Draft'
  if (Date.parse(report.expiresAt) <= now) return 'Checks expired'
  if (report.pairs.some(pair => pair.issues.length)) return 'Checks blocked'
  return report.phase === 'read' ? 'Ready for active checks' : 'Reviewed'
}

export type MigrationAction = 'start' | 'cutover' | 'pause' | 'resume' | 'stop' | 'retry'
  | 'removeIncomplete' | 'deleteSources' | 'deleteRecord'

export type MigrationRunStatus = 'copying' | 'cuttingOver' | 'awaitingCutover' | 'paused' | 'stopped' | 'failed' | 'verified'
export type MigrationBucketStatus = 'pending' | 'preparing' | 'copying' | 'awaitingCutover' | 'verifying' | 'verified' | 'failed' | 'removed'

export interface MigrationRun {
  status: MigrationRunStatus
  buckets: {
    mapping: MigrationMapping
    status: MigrationBucketStatus
    completed: number
    total: number
    currentKey: string | null
    error: string | null
    cleanupError: string | null
    sourceDeleted: boolean
    owned: boolean
    sourceProtected: boolean
  }[]
}

export const runLabels: Record<MigrationRunStatus, string> = {
  copying: 'Pre-copying', cuttingOver: 'Cutting over', awaitingCutover: 'Ready for cutover',
  paused: 'Paused', stopped: 'Stopped', failed: 'Failed buckets', verified: 'Verified',
}

export const bucketLabels: Record<MigrationBucketStatus, string> = {
  pending: 'Pending', preparing: 'Preparing', copying: 'Copying', awaitingCutover: 'Pre-copied',
  verifying: 'Verifying', verified: 'Verified', failed: 'Failed', removed: 'Destination removed',
}
