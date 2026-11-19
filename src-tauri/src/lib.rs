mod administration;
mod audit;
mod bucket;
mod common;
mod connection;
#[cfg(target_os = "macos")]
mod dock_icon;
mod monitoring;
mod object;
mod portal;
mod transfer;

use std::sync::Arc;

use tauri::{Manager, WebviewWindowBuilder};

use administration::{
    controller as administration_commands, platform_governance::controller as governance_commands,
    rgw_identities::RgwIdentityService, service::AdministrationService,
    workspace_settings::controller as settings_commands,
};
use audit::{controller as audit_commands, operational_events::controller as operational_commands, service::AuditService};
use bucket::controller as bucket_commands;
use connection::shared_service::{controller as shared_service_commands, service::SharedService};
use connection::{
    controller as connection_commands, repository::ConnectionRepository, service::ConnectionService,
};
use monitoring::{
    controller as monitoring_commands,
    service::MonitoringService,
    storage_costs::{StorageCosts, controller as cost_commands},
};
use object::controller as object_commands;
use transfer::{
    controller as transfer_commands, repository::JobRepository, service::TransferService,
};

pub fn run() {
    tracing_subscriber::fmt().with_target(false).init();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            let connections = Arc::new(ConnectionService::new(ConnectionRepository::open(
                directory.clone(),
            )?)?);
            let transfers = TransferService::new(
                JobRepository::open(directory.clone())?,
                Arc::clone(&connections),
            );
            app.manage(SharedService::open(directory.clone())?);
            app.manage(connections);
            app.manage(transfers);
            app.manage(transfer::migration::service::MigrationService::open(
                directory.clone(),
            )?);
            app.manage(MonitoringService::open(directory.clone())?);
            app.manage(StorageCosts::new(directory.clone()));
            app.manage(AuditService::open(directory.clone())?);
            app.manage(administration::private_access::PrivateAccessService::new(
                directory.clone(),
            ));
            app.manage(RgwIdentityService::new(directory));
            app.manage(AdministrationService::new()?);
            create_main_window(app)?;
            start_rgw_expiration_worker(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connection_commands::list_connections,
            connection_commands::list_openstack_clouds,
            connection_commands::save_openstack_cloud,
            connection_commands::disconnect_openstack_cloud,
            connection_commands::authenticate_openstack,
            connection_commands::discover_openstack_project,
            connection_commands::activate_openstack_project,
            connection_commands::save_connection,
            connection_commands::remove_connection,
            connection_commands::test_connection,
            shared_service_commands::list_shared_services,
            shared_service_commands::save_shared_service,
            shared_service_commands::connect_shared_service,
            shared_service_commands::disconnect_shared_service,
            shared_service_commands::read_shared_contexts,
            shared_service_commands::open_shared_action,
            portal::controller::read_portal,
            portal::controller::change_portal,
            portal::controller::portal_preview,
            portal::controller::portal_download,
            portal::controller::portal_upload,
            bucket_commands::list_buckets,
            bucket_commands::bucket_workbench_inventory,
            bucket_commands::bucket_workbench_configuration,
            bucket_commands::apply_bucket_campaign_configuration,
            bucket_commands::bucket_campaign_quota,
            bucket_commands::apply_bucket_campaign_quota,
            bucket_commands::recalculate_bucket_campaign_usage,
            bucket_commands::export_bucket_inventory,
            bucket_commands::create_bucket,
            bucket_commands::finish_bucket_versioning,
            bucket_commands::delete_bucket,
            bucket_commands::configure_bucket,
            bucket_commands::export_bucket_configuration,
            bucket_commands::import_bucket_backup,
            bucket_commands::read_bucket_quota,
            bucket_commands::update_bucket_quota,
            object_commands::list_objects,
            object_commands::list_versions,
            object_commands::object_details,
            object_commands::object_tags,
            object_commands::object_acl,
            object_commands::preview_object,
            object_commands::share_object,
            object_commands::create_folder,
            object_commands::change_object,
            object_commands::list_multipart,
            object_commands::abort_multipart,
            transfer_commands::start_transfer,
            transfer_commands::list_jobs,
            transfer_commands::prefix_history,
            transfer_commands::preview_version_plan,
            transfer_commands::export_transfer_report,
            transfer_commands::cancel_job,
            transfer_commands::compare_locations,
            transfer_commands::compare_buckets,
            transfer_commands::remediate_bucket_difference,
            transfer_commands::location_usage,
            transfer_commands::check_integrity,
            transfer_commands::preview_bucket_purge,
            transfer_commands::list_migrations,
            transfer_commands::save_migration,
            transfer_commands::check_migration,
            transfer_commands::restore_migration_access,
            transfer_commands::migration_action,
            governance_commands::list_platform_access,
            settings_commands::workspace_policy_settings,
            settings_commands::workspace_policy_resources,
            settings_commands::save_workspace_resource_policy,
            settings_commands::workspace_cost_settings,
            settings_commands::workspace_readiness,
            settings_commands::workspace_policy_accounts,
            settings_commands::workspace_portal_account,
            settings_commands::workspace_rotation_endpoints,
            settings_commands::rotate_workspace_keys,
            settings_commands::recover_workspace_rotation,
            governance_commands::export_platform_access,
            governance_commands::list_platform_groups,
            governance_commands::list_platform_connections,
            governance_commands::list_private_platform_connections,
            governance_commands::save_platform_group,
            governance_commands::rename_platform_connection,
            administration_commands::administer,
            administration_commands::list_private_iam,
            administration_commands::change_private_iam,
            administration_commands::read_rgw_identity,
            administration_commands::change_rgw_identity,
            administration_commands::ceph_action,
            administration_commands::ceph_link_owner,
            administration_commands::ceph_index_batch,
            administration_commands::ceph_multisite_topology,
            administration_commands::ceph_multisite_flow,
            administration_commands::ceph_multisite_replication,
            administration_commands::ceph_multisite_journal,
            administration_commands::export_ceph_journal,
            audit_commands::list_audit,
            operational_commands::operational_events,
            operational_commands::operational_inbox,
            operational_commands::read_operational_notifications,
            operational_commands::operational_quota_preferences,
            operational_commands::test_operational_smtp,
            operational_commands::operational_webhooks,
            operational_commands::operational_webhook_deliveries,
            operational_commands::operational_health,
            operational_commands::operational_usage_history,
            operational_commands::operational_bucket_usage,
            operational_commands::operational_cost_history,
            operational_commands::collect_operational_usage_history,
            monitoring_commands::monitoring_cached,
            monitoring_commands::monitoring_collect,
            monitoring_commands::monitoring_check,
            cost_commands::storage_cost_report,
            cost_commands::storage_cost_settings,
            cost_commands::storage_cost_collect,
            cost_commands::storage_cost_summary,
            cost_commands::storage_cost_export,
        ]);
    #[cfg(target_os = "macos")]
    let builder = builder.on_window_event(dock_icon::on_window_event);
    let result = builder
        .build(tauri::generate_context!())
        .map(|app| app.run(on_app_event));
    if let Err(error) = result {
        tracing::error!(%error, "StoreForge could not start");
        std::process::exit(1);
    }
}

fn start_rgw_expiration_worker(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let service = app.state::<RgwIdentityService>();
            let connections = app.state::<Arc<ConnectionService>>();
            if let Err(error) = service.enforce_expirations(&connections).await {
                tracing::error!(%error, "Could not update RGW expiration records");
            }
        }
    });
}

fn on_app_event(_app: &tauri::AppHandle, _event: tauri::RunEvent) {
    #[cfg(target_os = "macos")]
    dock_icon::on_app_event(_app, _event);
}

fn create_main_window(app: &tauri::App) -> tauri::Result<()> {
    let overlay = cfg!(target_os = "macos");
    let chrome = if overlay { "overlay" } else { "custom" };
    WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
        .decorations(overlay)
        .initialization_script(format!("window.__storeforgeWindowChrome = '{chrome}'"))
        .build()?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/integration/s3.rs"]
mod s3_integration;
