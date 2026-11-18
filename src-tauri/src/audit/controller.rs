use tauri::State;

use crate::common::error::Result;

use super::{model::AuditEvent, service::AuditService};

#[tauri::command]
pub fn list_audit(service: State<'_, AuditService>) -> Result<Vec<AuditEvent>> {
    service.list()
}
