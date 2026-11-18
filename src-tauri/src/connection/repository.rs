use std::{path::PathBuf, sync::Mutex};

use crate::common::{
    document::{read_document, write_document},
    error::{AppError, Result},
};

use super::model::StoredConnection;

pub struct ConnectionRepository {
    directory: PathBuf,
    profiles: Mutex<Vec<StoredConnection>>,
}

impl ConnectionRepository {
    pub fn open(directory: PathBuf) -> Result<Self> {
        let profiles = read_document(&directory.join("connections.json"))?;
        Ok(Self {
            directory,
            profiles: Mutex::new(profiles),
        })
    }

    pub fn read<T>(&self, query: impl FnOnce(&[StoredConnection]) -> Result<T>) -> Result<T> {
        let profiles = self
            .profiles
            .lock()
            .map_err(|_| AppError::File("Connections are unavailable.".into()))?;
        query(&profiles)
    }

    pub fn update<T>(
        &self,
        change: impl FnOnce(&mut Vec<StoredConnection>) -> Result<T>,
    ) -> Result<T> {
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| AppError::File("Connections are unavailable.".into()))?;
        let mut updated = profiles.clone();
        let result = change(&mut updated)?;
        write_document(
            &self.directory,
            &self.directory.join("connections.json"),
            &updated,
        )?;
        *profiles = updated;
        Ok(result)
    }
}
