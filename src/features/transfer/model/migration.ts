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
  recoveryRequired: boolean
  recoveryError: string | null
}

export function migrationStatus(migration: Migration, now: number): string {
  if (migration.checking) return migration.checking === 'active' ? 'Running active checks' : 'Running read-only checks'
  if (migration.recoveryRequired) return 'Restore access'
  const report = migration.report
  if (!report || report.revision !== migration.revision) return 'Draft'
  if (Date.parse(report.expiresAt) <= now) return 'Checks expired'
  if (report.pairs.some(pair => pair.issues.length)) return 'Checks blocked'
  return report.phase === 'read' ? 'Ready for active checks' : 'Reviewed'
}
