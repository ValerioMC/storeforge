import { desktop } from '../../../common/tauri'
import type { Migration, MigrationAction, MigrationDraft, MigrationPhase } from '../model/migration'

export const migrationsApi = {
  list(source: string): Promise<Migration[]> {
    return desktop('list_migrations', { source })
  },
  save(draft: MigrationDraft, record: Migration | undefined): Promise<Migration> {
    return desktop('save_migration', { draft, id: record?.id ?? null, revision: record?.revision ?? null })
  },
  check(record: Migration, phase: MigrationPhase, approved = false): Promise<Migration> {
    return desktop('check_migration', { id: record.id, revision: record.revision, phase, approved })
  },
  action(record: Migration, action: MigrationAction, confirmation: string | null): Promise<Migration | null> {
    return desktop('migration_action', { id: record.id, revision: record.revision, action, confirmation })
  },
  restore(record: Migration): Promise<Migration> {
    return desktop('restore_migration_access', { id: record.id, revision: record.revision })
  },
}
