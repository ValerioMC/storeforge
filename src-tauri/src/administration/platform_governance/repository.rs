use reqwest::Method;
use uuid::Uuid;

use crate::{
    common::error::Result,
    connection::shared_service::{model::ServiceSession, service::SharedService},
};

use super::model::{
    AccessFilters, AccessQuery, AccessRow, ConnectionName, GovernancePage, Group, GroupDraft,
    ListingQuery, ManagedConnection, PersonalConnection,
};

pub(super) struct GovernanceRepository<'a> {
    shared: &'a SharedService,
    id: Uuid,
}

impl<'a> GovernanceRepository<'a> {
    pub fn new(shared: &'a SharedService, id: Uuid) -> Self {
        Self { shared, id }
    }

    pub async fn access(&self, query: &AccessQuery<'_>) -> Result<GovernancePage<AccessRow>> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    remote
                        .get(origin, "/api/admin/access-audit", query, Some(token))
                        .await
                },
            )
            .await
    }

    pub async fn export_access(&self, filters: &AccessFilters) -> Result<String> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    remote
                        .csv(origin, "/api/admin/access-audit/export.csv", filters, token)
                        .await
                },
            )
            .await
    }

    pub async fn groups(&self, query: &ListingQuery) -> Result<GovernancePage<Group>> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    remote
                        .get(origin, "/api/admin/groups", query, Some(token))
                        .await
                },
            )
            .await
    }

    pub async fn managed_connections(
        &self,
        query: &ListingQuery,
    ) -> Result<GovernancePage<ManagedConnection>> {
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    remote
                        .get(origin, "/api/admin/s3-connections", query, Some(token))
                        .await
                },
            )
            .await
    }

    pub async fn personal_connections(&self) -> Result<Vec<PersonalConnection>> {
        self.shared
            .scoped(
                self.id,
                |_| true,
                async |remote, origin, token| {
                    remote
                        .get(
                            origin,
                            "/api/connections",
                            &[] as &[(&str, &str)],
                            Some(token),
                        )
                        .await
                },
            )
            .await
    }

    pub async fn save_group(&self, group_id: Option<i64>, draft: &GroupDraft) -> Result<Group> {
        let (method, path) = match group_id {
            Some(id) => (Method::PATCH, format!("/api/admin/groups/{id}")),
            None => (Method::POST, "/api/admin/groups".into()),
        };
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    remote.change(method, origin, &path, token, draft).await
                },
            )
            .await
    }

    pub async fn rename_connection(
        &self,
        connection_id: i64,
        name: &str,
    ) -> Result<ManagedConnection> {
        let path = format!("/api/admin/s3-connections/{connection_id}");
        self.shared
            .scoped(
                self.id,
                ServiceSession::admin_available,
                async |remote, origin, token| {
                    remote
                        .change(
                            Method::PATCH,
                            origin,
                            &path,
                            token,
                            &ConnectionName { name },
                        )
                        .await
                },
            )
            .await
    }
}
