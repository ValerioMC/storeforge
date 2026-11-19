use std::collections::BTreeMap;

use serde_json::Value;
use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::shared_service::service::SharedService,
};

use super::{
    model::{
        AccountOverride, AccountPortalSettings, FeatureLock, PolicyAccounts, Readiness,
        ResourceKind, ResourceOverride, ResourcePayload, ResourcePolicies, ResourcePolicy,
        ResourcePolicyItem, RotationEndpoint, RotationKind, RotationPlan, RotationRecovery,
        RotationReport, ServiceCostSettings, SettingEdit, SettingGroup, SettingValue,
        SettingsSection, WorkspaceSettings,
    },
    repository::SettingsRepository,
};

pub struct WorkspaceSettingsService<'a> {
    repository: SettingsRepository<'a>,
}

impl<'a> WorkspaceSettingsService<'a> {
    pub fn new(shared: &'a SharedService, id: Uuid) -> Self {
        Self {
            repository: SettingsRepository::new(shared, id),
        }
    }

    pub async fn read(&self, section: SettingsSection) -> Result<WorkspaceSettings> {
        self.repository
            .settings(
                section,
                |_, _| Ok(false),
                |document, locks| project(section, document, locks),
            )
            .await
    }

    pub async fn save(
        &self,
        section: SettingsSection,
        edits: Vec<SettingEdit>,
    ) -> Result<WorkspaceSettings> {
        self.repository
            .settings(
                section,
                |document, locks| {
                    let view = project(section, document, locks.clone())?;
                    for edit in edits {
                        apply_edit(document, section, &view, edit)?;
                    }
                    Ok(true)
                },
                |document, locks| project(section, document, locks),
            )
            .await
    }

    pub async fn readiness(&self) -> Result<Readiness> {
        self.repository.readiness().await
    }

    pub async fn accounts(&self, page: u32, search: &str) -> Result<PolicyAccounts> {
        self.repository.accounts(page, search).await
    }

    pub async fn portal_account(
        &self,
        account: u64,
        change: Option<&AccountOverride>,
    ) -> Result<AccountPortalSettings> {
        match change {
            Some(change) => self.repository.save_portal_account(account, change).await,
            None => self.repository.portal_account(account).await,
        }
    }

    pub async fn resources(
        &self,
        kind: ResourceKind,
        page: u32,
        search: &str,
    ) -> Result<ResourcePolicies> {
        let response = self.repository.resources(kind, page, search).await?;
        Ok(ResourcePolicies {
            items: response
                .items
                .into_iter()
                .map(|item| policy_item(kind, item))
                .collect(),
            has_next: response.has_next,
        })
    }

    pub async fn save_resource(
        &self,
        kind: ResourceKind,
        resource: u64,
        change: &ResourceOverride,
    ) -> Result<ResourcePolicyItem> {
        if !change.policy.available(kind) {
            return Err(AppError::Invalid(
                "This policy does not apply to the selected resource.".into(),
            ));
        }
        let updated = self
            .repository
            .save_resource(kind, resource, change)
            .await?;
        Ok(policy_item(kind, updated))
    }

    pub async fn cost_settings(
        &self,
        draft: Option<&ServiceCostSettings>,
    ) -> Result<ServiceCostSettings> {
        self.repository.cost_settings(draft).await
    }

    pub async fn endpoints(&self) -> Result<Vec<RotationEndpoint>> {
        self.repository.endpoints().await
    }

    pub async fn rotate(&self, plan: RotationPlan) -> Result<RotationReport> {
        validate_plan(&plan)?;
        self.repository.rotate(&plan).await
    }

    pub async fn recover(&self, recovery: RotationRecovery) -> Result<RotationReport> {
        self.repository
            .recover(recovery.endpoint_id, |endpoint| {
                recovery_plan(endpoint, recovery)
            })
            .await
    }
}

fn project(
    section: SettingsSection,
    document: &Value,
    locks: BTreeMap<String, FeatureLock>,
) -> Result<WorkspaceSettings> {
    let values = section_object(document, section)?;
    let mut groups = vec![SettingGroup {
        name: None,
        values: scalar_settings(values, section, None),
    }];
    groups.extend(values.iter().filter_map(|(key, value)| {
        let group = value.as_object()?;
        let values = scalar_settings(group, section, Some(key));
        (!values.is_empty()).then(|| SettingGroup {
            name: Some(key.clone()),
            values,
        })
    }));
    Ok(WorkspaceSettings { groups, locks })
}

fn validate_plan(plan: &RotationPlan) -> Result<()> {
    if plan.endpoint_ids.is_empty() || plan.endpoint_ids.contains(&0) || plan.key_types.is_empty() {
        return Err(AppError::Invalid(
            "Select endpoints and key categories.".into(),
        ));
    }
    if plan.deactivate_only && plan.key_types.iter().any(|kind| kind.requires_delete()) {
        return Err(AppError::Invalid(
            "Managed technical keys require deletion after validation.".into(),
        ));
    }
    Ok(())
}

fn recovery_plan(endpoint: RotationEndpoint, recovery: RotationRecovery) -> Result<RotationPlan> {
    let pending = recovery.kind.identity().and_then(|kind| {
        endpoint
            .service_identities
            .iter()
            .find(|identity| identity.kind == kind)
    });
    if endpoint.id != recovery.endpoint_id || endpoint.provider != "ceph" {
        return Err(AppError::Invalid("The rotation endpoint changed.".into()));
    }
    let Some(identity) = pending.filter(|identity| identity.rotation_phase == Some(recovery.phase))
    else {
        return Err(AppError::Invalid(
            "The journal has changed. Refresh before starting another rotation.".into(),
        ));
    };
    if (recovery.kind == RotationKind::CephAdmin
        && (!endpoint.is_editable || identity.mode != "external"))
        || (recovery.kind.requires_delete() && identity.mode != "managed")
    {
        return Err(AppError::Invalid(
            "The deployment controls this credential.".into(),
        ));
    }
    let plan = RotationPlan {
        endpoint_ids: vec![endpoint.id],
        key_types: vec![recovery.kind],
        deactivate_only: recovery.deactivate_only,
    };
    validate_plan(&plan)?;
    Ok(plan)
}

fn section_object(
    document: &Value,
    section: SettingsSection,
) -> Result<&serde_json::Map<String, Value>> {
    document[section.key()]
        .as_object()
        .ok_or_else(|| AppError::Storage("The service did not publish these settings.".into()))
}

fn scalar_settings(
    values: &serde_json::Map<String, Value>,
    section: SettingsSection,
    group: Option<&str>,
) -> super::model::ScalarSettings {
    values
        .iter()
        .filter_map(|(key, value)| {
            if !editable_field(section, group, key) {
                return None;
            }
            let value = serde_json::from_value(value.clone()).ok()?;
            Some((key.clone(), value))
        })
        .collect()
}

fn editable_field(section: SettingsSection, group: Option<&str>, key: &str) -> bool {
    match section {
        SettingsSection::General => match group {
            Some("project_feature_policies" | "project_collection_policies") => true,
            None => key.ends_with("_enabled"),
            _ => false,
        },
        SettingsSection::Manager => matches!(group, None | Some("feature_policies")),
        SettingsSection::Browser => group.is_none(),
        SettingsSection::Portal => {
            matches!(group, None | Some("feature_policies" | "bucket_defaults"))
        }
        SettingsSection::QuotaNotifications => {
            group.is_none() && matches!(key, "threshold_percent" | "include_subject_contact_email")
        }
    }
}

fn apply_edit(
    document: &mut Value,
    section: SettingsSection,
    view: &WorkspaceSettings,
    edit: SettingEdit,
) -> Result<()> {
    let is_locked =
        edit.group.is_none() && view.locks.get(&edit.field).is_some_and(|lock| lock.forced);
    let current = view
        .groups
        .iter()
        .find(|group| group.name == edit.group)
        .and_then(|group| group.values.get(&edit.field));
    let Some(current) = current.filter(|_| !is_locked) else {
        return Err(AppError::Invalid(
            "This setting is unavailable or locked.".into(),
        ));
    };
    if current != &edit.before {
        return Err(AppError::Invalid(
            "This setting changed in the service. Refresh it before saving.".into(),
        ));
    }
    if std::mem::discriminant(current) != std::mem::discriminant::<SettingValue>(&edit.value) {
        return Err(AppError::Invalid(
            "The setting value has the wrong type.".into(),
        ));
    }
    let section = &mut document[section.key()];
    let values = match edit.group {
        Some(group) => &mut section[group],
        None => section,
    };
    values[edit.field] = serde_json::to_value(edit.value)?;
    Ok(())
}

fn policy_item(kind: ResourceKind, resource: ResourcePayload) -> ResourcePolicyItem {
    let overrides = resource
        .fields
        .into_iter()
        .filter_map(|(key, value)| {
            let policy: ResourcePolicy = serde_json::from_value(Value::String(key)).ok()?;
            let value = serde_json::from_value(value).ok()?;
            policy.available(kind).then_some((policy, value))
        })
        .collect();
    ResourcePolicyItem {
        id: resource.id,
        name: resource.name,
        overrides,
    }
}
