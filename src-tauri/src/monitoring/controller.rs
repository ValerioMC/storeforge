use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::{common::error::Result, connection::service::ConnectionService};

use super::{
    model::{CheckMode, MonitoringView},
    service::MonitoringService,
};

#[tauri::command]
pub fn monitoring_cached(
    monitoring: State<'_, MonitoringService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
) -> Result<MonitoringView> {
    monitoring.cached(&connections, connection)
}

#[tauri::command]
pub async fn monitoring_collect(
    monitoring: State<'_, MonitoringService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
) -> Result<MonitoringView> {
    monitoring.collect(&connections, connection).await
}

#[tauri::command]
pub async fn monitoring_check(
    monitoring: State<'_, MonitoringService>,
    connections: State<'_, Arc<ConnectionService>>,
    connection: Uuid,
    mode: CheckMode,
) -> Result<MonitoringView> {
    monitoring.check(&connections, connection, mode).await
}
