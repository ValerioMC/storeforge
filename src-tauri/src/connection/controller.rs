use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::common::error::Result;

use super::{
    model::{Connection, ConnectionCheck, ConnectionDraft},
    openstack::model::{
        Activation, Authentication, Cloud, CloudSettings, CloudStatus, ProjectAccess, SessionView,
    },
    service::ConnectionService,
};

#[tauri::command]
pub fn list_connections(service: State<'_, Arc<ConnectionService>>) -> Result<Vec<Connection>> {
    service.list()
}

#[tauri::command]
pub async fn save_connection(
    service: State<'_, Arc<ConnectionService>>,
    draft: ConnectionDraft,
) -> Result<Connection> {
    service.save(draft).await
}

#[tauri::command]
pub fn remove_connection(service: State<'_, Arc<ConnectionService>>, id: Uuid) -> Result<()> {
    service.remove(id)
}

#[tauri::command]
pub async fn test_connection(
    service: State<'_, Arc<ConnectionService>>,
    draft: ConnectionDraft,
) -> Result<ConnectionCheck> {
    service.check(draft).await
}

#[tauri::command]
pub fn list_openstack_clouds(
    service: State<'_, Arc<ConnectionService>>,
) -> Result<Vec<CloudStatus>> {
    service.openstack.list()
}

#[tauri::command]
pub fn save_openstack_cloud(
    service: State<'_, Arc<ConnectionService>>,
    id: Option<Uuid>,
    settings: CloudSettings,
) -> Result<Cloud> {
    service.openstack.save(id, settings)
}

#[tauri::command]
pub fn disconnect_openstack_cloud(
    service: State<'_, Arc<ConnectionService>>,
    id: Uuid,
    forget: bool,
) -> Result<()> {
    service.openstack.disconnect(id, forget)
}

#[tauri::command]
pub async fn authenticate_openstack(
    service: State<'_, Arc<ConnectionService>>,
    id: Uuid,
    authentication: Authentication,
) -> Result<SessionView> {
    service.openstack.authenticate(id, authentication).await
}

#[tauri::command]
pub async fn discover_openstack_project(
    service: State<'_, Arc<ConnectionService>>,
    id: Uuid,
    project_id: String,
) -> Result<ProjectAccess> {
    service.openstack.discover(id, project_id).await
}

#[tauri::command]
pub async fn activate_openstack_project(
    service: State<'_, Arc<ConnectionService>>,
    id: Uuid,
    selection: Activation,
) -> Result<Connection> {
    service.openstack.activate(id, selection).await
}
