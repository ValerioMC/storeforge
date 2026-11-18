use std::path::PathBuf;

use chrono::Utc;
use uuid::Uuid;

use crate::common::error::Result;

use super::{model::AuditEvent, repository::AuditRepository};

pub struct AuditService {
    repository: AuditRepository,
}

impl AuditService {
    pub fn open(directory: PathBuf) -> Result<Self> {
        Ok(Self {
            repository: AuditRepository::open(directory)?,
        })
    }

    pub fn list(&self) -> Result<Vec<AuditEvent>> {
        self.repository.list()
    }

    pub fn record<T>(
        &self,
        connection: Uuid,
        action: &str,
        resource: Option<String>,
        result: Result<T>,
    ) -> Result<T> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            connection,
            action: action.into(),
            resource,
            success: result.is_ok(),
        };
        if let Err(error) = self.repository.insert(event) {
            tracing::error!(%error, %connection, %action, "Could not persist audit event");
        }
        result
    }
}
