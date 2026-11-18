use std::{path::PathBuf, sync::Mutex};

use crate::common::{
    document::{read_document, write_document},
    error::{AppError, Result},
};

use super::model::AuditEvent;

pub struct AuditRepository {
    directory: PathBuf,
    events: Mutex<Vec<AuditEvent>>,
}

impl AuditRepository {
    pub fn open(directory: PathBuf) -> Result<Self> {
        let events = read_document(&directory.join("audit.json"))?;
        Ok(Self {
            directory,
            events: Mutex::new(events),
        })
    }

    pub fn list(&self) -> Result<Vec<AuditEvent>> {
        Ok(self.events.lock().map_err(|_| unavailable())?.clone())
    }

    pub fn insert(&self, event: AuditEvent) -> Result<()> {
        let mut events = self.events.lock().map_err(|_| unavailable())?;
        let mut updated = events.clone();
        updated.insert(0, event);
        updated.truncate(1000);
        write_document(
            &self.directory,
            &self.directory.join("audit.json"),
            &updated,
        )?;
        *events = updated;
        Ok(())
    }
}

fn unavailable() -> AppError {
    AppError::File("The audit history is unavailable.".into())
}
