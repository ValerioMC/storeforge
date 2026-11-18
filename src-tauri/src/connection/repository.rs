use std::{path::PathBuf, sync::Mutex};

use crate::common::{
    document::{read_document, write_document},
    error::{AppError, Result},
};

use uuid::Uuid;

use super::model::{Secret, StoredConnection};

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

pub trait CredentialRepository: Send + Sync {
    fn read(&self, id: Uuid) -> Result<Secret>;
    fn write(&self, id: Uuid, secret: &Secret) -> Result<()>;
    fn remove(&self, id: Uuid);
}

pub struct KeyringRepository;

impl CredentialRepository for KeyringRepository {
    fn read(&self, id: Uuid) -> Result<Secret> {
        Ok(serde_json::from_str(
            &credential_entry(id)?.get_password()?,
        )?)
    }

    fn write(&self, id: Uuid, secret: &Secret) -> Result<()> {
        credential_entry(id)?.set_password(&serde_json::to_string(secret)?)?;
        Ok(())
    }

    fn remove(&self, id: Uuid) {
        let result = credential_entry(id).and_then(|entry| match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        });
        if let Err(error) = result {
            tracing::warn!(%error, "Could not remove unused credentials");
        }
    }
}

fn credential_entry(id: Uuid) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(
        "dev.vmcforge.storeforge",
        &id.to_string(),
    )?)
}
