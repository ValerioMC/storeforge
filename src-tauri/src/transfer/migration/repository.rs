use std::{path::PathBuf, sync::Mutex};

use chrono::Utc;
use uuid::Uuid;

use crate::common::{
    document::{read_document, write_document},
    error::{AppError, Result},
};

use super::model::{Migration, MigrationView};

pub struct MigrationRepository {
    directory: PathBuf,
    records: Mutex<Vec<Migration>>,
}

impl MigrationRepository {
    pub fn open(directory: PathBuf) -> Result<Self> {
        let mut records: Vec<Migration> = read_document(&directory.join("migrations.json"))?;
        for record in &mut records {
            if let Some(run) = &mut record.run {
                run.reconcile_after_restart();
            }
            if record.checking.take().is_some() {
                record.report = None;
                if !record.private.effects.is_empty() {
                    record.recovery_error =
                        Some("Checks were interrupted. Restore access before continuing.".into());
                }
            }
        }
        Ok(Self {
            directory,
            records: Mutex::new(records),
        })
    }

    pub fn list(&self, source: Uuid) -> Result<Vec<MigrationView>> {
        let records = self.records.lock().map_err(|_| unavailable())?;
        Ok(records
            .iter()
            .filter(|row| row.draft.source == source)
            .map(Migration::view)
            .collect())
    }

    pub fn get(&self, id: Uuid) -> Result<Migration> {
        let records = self.records.lock().map_err(|_| unavailable())?;
        records
            .iter()
            .find(|row| row.id == id)
            .cloned()
            .ok_or_else(|| AppError::Missing("The saved migration no longer exists.".into()))
    }

    pub fn save(&self, record: &mut Migration) -> Result<()> {
        let mut records = self.records.lock().map_err(|_| unavailable())?;
        record.updated_at = Utc::now();
        let mut updated = records.clone();
        updated.retain(|row| row.id != record.id);
        updated.insert(0, record.clone());
        self.persist(&updated)?;
        *records = updated;
        Ok(())
    }

    pub fn remove(&self, id: Uuid) -> Result<()> {
        let mut records = self.records.lock().map_err(|_| unavailable())?;
        let mut updated = records.clone();
        updated.retain(|row| row.id != id);
        self.persist(&updated)?;
        *records = updated;
        Ok(())
    }

    fn persist(&self, records: &[Migration]) -> Result<()> {
        write_document(
            &self.directory,
            &self.directory.join("migrations.json"),
            records,
        )
    }
}

fn unavailable() -> AppError {
    AppError::File("The saved migration inventory is unavailable.".into())
}
