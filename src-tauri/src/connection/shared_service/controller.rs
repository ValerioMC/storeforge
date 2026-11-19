use tauri::State;
use uuid::Uuid;

use crate::common::error::{AppError, Result};

use super::{
    model::{
        InteractiveAction, ServiceDraft, ServiceProfile, ServiceSession, ServiceStatus,
        SharedContext, SharedWorkspace,
    },
    service::SharedService,
};

#[tauri::command]
pub fn list_shared_services(service: State<'_, SharedService>) -> Result<Vec<ServiceStatus>> {
    service.list()
}

#[tauri::command]
pub fn save_shared_service(
    service: State<'_, SharedService>,
    id: Option<Uuid>,
    draft: ServiceDraft,
) -> Result<ServiceProfile> {
    service.save(id, draft)
}

#[tauri::command]
pub async fn connect_shared_service(
    service: State<'_, SharedService>,
    id: Uuid,
    token: String,
) -> Result<ServiceSession> {
    service.connect(id, token).await
}

#[tauri::command]
pub fn disconnect_shared_service(
    service: State<'_, SharedService>,
    id: Uuid,
    forget: bool,
) -> Result<()> {
    service.disconnect(id, forget)
}

#[tauri::command]
pub async fn read_shared_contexts(
    service: State<'_, SharedService>,
    id: Uuid,
    workspace: SharedWorkspace,
) -> Result<Vec<SharedContext>> {
    service.contexts(id, workspace).await
}

#[tauri::command]
pub async fn open_shared_action(
    service: State<'_, SharedService>,
    id: Uuid,
    action: InteractiveAction,
) -> Result<()> {
    let url = service.interactive_url(id, action).await?;
    open::that_detached(url.as_str())
        .map_err(|_| AppError::File("Could not open your system browser.".into()))
}
