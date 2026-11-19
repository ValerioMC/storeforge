use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::shared_service::service::SharedService,
};

use super::{
    model::{
        AccessFilters, AccessQuery, AccessRow, GovernancePage, Group, GroupDraft, ListingQuery,
        ManagedConnection, PersonalConnection,
    },
    repository::GovernanceRepository,
};

pub struct GovernanceService<'a> {
    repository: GovernanceRepository<'a>,
}

impl<'a> GovernanceService<'a> {
    pub fn new(shared: &'a SharedService, id: Uuid) -> Self {
        Self {
            repository: GovernanceRepository::new(shared, id),
        }
    }

    pub async fn access(
        &self,
        page: u32,
        filters: &AccessFilters,
    ) -> Result<GovernancePage<AccessRow>> {
        self.repository.access(&AccessQuery { page, filters }).await
    }

    pub async fn export_access(&self, filters: &AccessFilters) -> Result<String> {
        self.repository.export_access(filters).await
    }

    pub async fn groups(&self, query: &ListingQuery) -> Result<GovernancePage<Group>> {
        self.repository.groups(query).await
    }

    pub async fn managed_connections(
        &self,
        query: &ListingQuery,
    ) -> Result<GovernancePage<ManagedConnection>> {
        self.repository.managed_connections(query).await
    }

    pub async fn private_connections(&self) -> Result<Vec<PersonalConnection>> {
        Ok(self
            .repository
            .personal_connections()
            .await?
            .into_iter()
            .filter(|connection| !connection.is_shared)
            .collect())
    }

    pub async fn save_group(&self, group_id: Option<i64>, draft: GroupDraft) -> Result<Group> {
        let draft = GroupDraft {
            name: valid_name(&draft.name)?.to_owned(),
            description: draft.description,
        };
        self.repository.save_group(group_id, &draft).await
    }

    pub async fn rename_connection(
        &self,
        connection_id: i64,
        name: &str,
    ) -> Result<ManagedConnection> {
        self.repository
            .rename_connection(connection_id, valid_name(name)?)
            .await
    }
}

fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Invalid("Enter a name.".into()));
    }
    Ok(name)
}
