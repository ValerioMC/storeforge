use std::collections::BTreeMap;

use reqwest::Method;
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::{
    common::error::Result,
    connection::shared_service::{model::ServiceSession, service::SharedService},
};

use super::model::{
    AccountOverride, AccountPortalSettings, FeatureLock, PolicyAccounts, Readiness, ResourceKind,
    ResourceOverride, ResourcePagePayload, ResourcePayload, RotationEndpoint, RotationPlan,
    RotationReport, ServiceCostSettings, SettingsSection,
};

pub struct SettingsRepository<'a> {
    shared: &'a SharedService,
    id: Uuid,
}

impl<'a> SettingsRepository<'a> {
    pub fn new(shared: &'a SharedService, id: Uuid) -> Self {
        Self { shared, id }
    }

    pub async fn settings<T>(
        &self,
        section: SettingsSection,
        prepare: impl FnOnce(&mut serde_json::Value, &BTreeMap<String, FeatureLock>) -> Result<bool>,
        project: impl FnOnce(&serde_json::Value, BTreeMap<String, FeatureLock>) -> Result<T>,
    ) -> Result<T> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    let query = &[] as &[(&str, &str)];
                    let path = "/api/admin/settings";
                    let mut document = remote.get(origin, path, query, Some(token)).await?;
                    let locks = if section == SettingsSection::General {
                        remote
                            .get(
                                origin,
                                "/api/admin/settings/general-feature-locks",
                                query,
                                Some(token),
                            )
                            .await?
                    } else {
                        BTreeMap::new()
                    };
                    if prepare(&mut document, &locks)? {
                        document = remote
                            .change(Method::PUT, origin, path, token, &document)
                            .await?;
                    }
                    project(&document, locks)
                },
            )
            .await
    }

    pub async fn readiness(&self) -> Result<Readiness> {
        self.read("/api/admin/production-readiness", &[] as &[(&str, &str)])
            .await
    }

    pub async fn endpoints(&self) -> Result<Vec<RotationEndpoint>> {
        self.read("/api/admin/storage-endpoints", &[] as &[(&str, &str)])
            .await
    }

    pub async fn rotate(&self, plan: &RotationPlan) -> Result<RotationReport> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    remote
                        .long_change(Method::POST, origin, "/api/admin/key-rotation", token, plan)
                        .await
                },
            )
            .await
    }

    pub async fn recover(
        &self,
        id: u64,
        prepare: impl FnOnce(RotationEndpoint) -> Result<RotationPlan>,
    ) -> Result<RotationReport> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    let endpoint = remote
                        .get(
                            origin,
                            &format!("/api/admin/storage-endpoints/{id}"),
                            &[] as &[(&str, &str)],
                            Some(token),
                        )
                        .await?;
                    let plan = prepare(endpoint)?;
                    remote
                        .long_change(
                            Method::POST,
                            origin,
                            "/api/admin/key-rotation",
                            token,
                            &plan,
                        )
                        .await
                },
            )
            .await
    }

    pub async fn accounts(&self, page: u32, search: &str) -> Result<PolicyAccounts> {
        self.read(
            "/api/admin/accounts",
            &[("page", page.to_string()), ("search", search.to_owned())],
        )
        .await
    }

    pub async fn portal_account(&self, id: u64) -> Result<AccountPortalSettings> {
        self.read(
            &format!("/api/admin/accounts/{id}/portal-settings"),
            &[] as &[(&str, &str)],
        )
        .await
    }

    pub async fn save_portal_account(
        &self,
        id: u64,
        change: &AccountOverride,
    ) -> Result<AccountPortalSettings> {
        let draft =
            serde_json::json!({ "override": BTreeMap::from([(&change.policy, change.value)]) });
        self.change(
            Method::PATCH,
            &format!("/api/admin/accounts/{id}/portal-settings"),
            &draft,
        )
        .await
    }

    pub async fn resources(
        &self,
        kind: ResourceKind,
        page: u32,
        search: &str,
    ) -> Result<ResourcePagePayload> {
        self.read(
            kind.path(),
            &[("page", page.to_string()), ("search", search.to_owned())],
        )
        .await
    }

    pub async fn save_resource(
        &self,
        kind: ResourceKind,
        id: u64,
        change: &ResourceOverride,
    ) -> Result<ResourcePayload> {
        let draft = BTreeMap::from([(&change.policy, change.value)]);
        self.change(Method::PATCH, &format!("{}/{id}", kind.path()), &draft)
            .await
    }

    pub async fn cost_settings(
        &self,
        draft: Option<&ServiceCostSettings>,
    ) -> Result<ServiceCostSettings> {
        let path = "/api/admin/settings/cost";
        match draft {
            Some(draft) => self.change(Method::PUT, path, draft).await,
            None => self.read(path, &[] as &[(&str, &str)]).await,
        }
    }

    async fn read<T: DeserializeOwned, Q: Serialize + ?Sized>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| remote.get(origin, path, query, Some(token)).await,
            )
            .await
    }

    async fn change<T: DeserializeOwned, D: Serialize>(
        &self,
        method: Method,
        path: &str,
        draft: &D,
    ) -> Result<T> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    remote.change(method, origin, path, token, draft).await
                },
            )
            .await
    }
}
