mod model;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use serde::de::DeserializeOwned;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    common::{
        document::{read_document, write_document},
        error::{AppError, Result},
    },
    connection::{
        model::{IdentityKind, ManagedIamOwner, Secret, Workspace},
        service::{ConnectionService, Session},
    },
};

use self::model::{
    Authority, CleanupPage, CleanupResponse, CreatedKey, CreatedUser, Members, ProvisioningRecord,
    RESERVED_PREFIX, ReadUser, User,
};
pub use self::model::{
    PrivateAccess, PrivateAccessChange, PrivateAccessDraft, PrivateAccessStatus,
};
use super::service::AdministrationService;

pub struct PrivateAccessService {
    directory: PathBuf,
    changes: Mutex<()>,
}

impl PrivateAccessService {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            changes: Mutex::new(()),
        }
    }

    pub async fn list(&self, connection: Uuid) -> Result<Vec<PrivateAccess>> {
        let _change = self.changes.lock().await;
        Ok(self
            .records()?
            .into_iter()
            .filter(|record| record.authority.connection == connection)
            .map(|record| record.view)
            .collect())
    }

    pub async fn change(
        &self,
        iam: &AdministrationService,
        connections: &ConnectionService,
        connection: Uuid,
        change: PrivateAccessChange,
    ) -> Result<Option<PrivateAccess>> {
        let _change = self.changes.lock().await;
        let session = connections.session_for(connection, Workspace::Manager, IdentityKind::S3)?;
        if session.profile.settings.openstack.is_some() {
            return Err(AppError::Invalid(
                "Use a direct administrator connection to provision IAM access.".into(),
            ));
        }
        if session.profile.settings.managed_iam.is_some() {
            return Err(protected_identity());
        }
        match change {
            PrivateAccessChange::Create { draft } => {
                validate_draft(&draft)?;
                let id = Uuid::new_v4();
                let mut record = ProvisioningRecord {
                    view: PrivateAccess {
                        id,
                        name: draft.name.trim().into(),
                        user_name: format!("{RESERVED_PREFIX}{id}"),
                        status: PrivateAccessStatus::Provisioning,
                        issue: None,
                    },
                    authority: authority(&session),
                    draft,
                    user_id: None,
                };
                self.store(&record)?;
                self.provision(iam, connections, &session, &mut record)
                    .await
            }
            PrivateAccessChange::Retry { id } | PrivateAccessChange::Retire { id } => {
                let mut record = self.record(id, &session)?;
                let retiring = matches!(change, PrivateAccessChange::Retire { .. })
                    || record.view.status == PrivateAccessStatus::Retiring;
                if !retiring && record.view.status == PrivateAccessStatus::Active {
                    return Err(AppError::Invalid(
                        "This connection is already active.".into(),
                    ));
                }
                if retiring {
                    record.view.status = PrivateAccessStatus::Retiring;
                    self.store(&record)?;
                }
                if let Err(error) = cleanup(iam, connections, &session, &record).await {
                    if !retiring {
                        record.view.status = PrivateAccessStatus::SetupIncomplete;
                    }
                    record.view.issue = Some(error.to_string());
                    self.store(&record)?;
                    return Ok(Some(record.view));
                }
                if retiring {
                    self.remove(id)?;
                    return Ok(None);
                }
                record.user_id = None;
                record.view.status = PrivateAccessStatus::Provisioning;
                record.view.issue = None;
                self.store(&record)?;
                self.provision(iam, connections, &session, &mut record)
                    .await
            }
        }
    }

    async fn provision(
        &self,
        iam: &AdministrationService,
        connections: &ConnectionService,
        session: &Session,
        record: &mut ProvisioningRecord,
    ) -> Result<Option<PrivateAccess>> {
        match self.setup(iam, connections, session, record).await {
            Ok(()) => record.view.status = PrivateAccessStatus::Active,
            Err(error) => {
                record.view.status = PrivateAccessStatus::SetupIncomplete;
                record.view.issue = Some(match cleanup(iam, connections, session, record).await {
                    Ok(()) => error.to_string(),
                    Err(cleanup_error) => {
                        format!("{error}. Cleanup requires a retry: {cleanup_error}")
                    }
                });
            }
        }
        self.store(record)?;
        Ok(Some(record.view.clone()))
    }

    async fn setup(
        &self,
        iam: &AdministrationService,
        connections: &ConnectionService,
        session: &Session,
        record: &mut ProvisioningRecord,
    ) -> Result<()> {
        let name = record.view.user_name.as_str();
        let result: CreatedUser = xml(&call(
            iam,
            session,
            "CreateUser",
            [("UserName", name), ("Path", &user_path(record.view.id))],
        )
        .await?)?;
        let user = result.result.user;
        assert_identity(record, &user)?;
        record.user_id = Some(user.user_id.clone());
        self.store(record)?;
        for group in &record.draft.groups {
            call(
                iam,
                session,
                "AddUserToGroup",
                [("UserName", name), ("GroupName", group)],
            )
            .await?;
        }
        for policy in &record.draft.policies {
            call(
                iam,
                session,
                "AttachUserPolicy",
                [("UserName", name), ("PolicyArn", policy)],
            )
            .await?;
        }
        for policy in &record.draft.inline_policies {
            call(
                iam,
                session,
                "PutUserPolicy",
                [
                    ("UserName", name),
                    ("PolicyName", &policy.name),
                    ("PolicyDocument", &policy.document),
                ],
            )
            .await?;
        }
        let response = call(iam, session, "CreateAccessKey", [("UserName", name)])
            .await
            .map_err(|_| {
                AppError::Storage("IAM could not issue an access key for this user.".into())
            })?;
        let key = xml::<CreatedKey>(&response)?.result.access_key;
        if !key.user_name.eq_ignore_ascii_case(name) {
            return Err(AppError::Storage(
                "IAM returned an access key for another user.".into(),
            ));
        }
        let owner = ManagedIamOwner {
            provisioning_id: record.view.id,
            administrator: record.authority.connection,
            user_id: user.user_id,
            user_name: user.user_name,
            access_key_id: key.access_key_id.clone(),
        };
        connections.save_managed_iam(
            &session.profile,
            record.view.name.clone(),
            owner,
            (record.draft.browser, record.draft.manager),
            Secret {
                access_key: key.access_key_id,
                secret_key: key.secret_access_key,
                session_token: None,
            },
        )?;
        Ok(())
    }

    fn records(&self) -> Result<Vec<ProvisioningRecord>> {
        read_document(&self.directory.join("iam-private-access.json"))
    }

    fn record(&self, id: Uuid, session: &Session) -> Result<ProvisioningRecord> {
        let record = self
            .records()?
            .into_iter()
            .find(|record| record.view.id == id)
            .ok_or_else(|| AppError::Missing("This private connection no longer exists.".into()))?;
        if record.authority != authority(session) {
            return Err(AppError::Invalid(
                "Use the original administrator and endpoint to recover this connection.".into(),
            ));
        }
        Ok(record)
    }

    fn store(&self, record: &ProvisioningRecord) -> Result<()> {
        let mut records = self.records()?;
        records.retain(|entry| entry.view.id != record.view.id);
        records.push(record.clone());
        self.write(&records)
    }

    fn remove(&self, id: Uuid) -> Result<()> {
        let mut records = self.records()?;
        records.retain(|entry| entry.view.id != id);
        self.write(&records)
    }

    fn write(&self, records: &[ProvisioningRecord]) -> Result<()> {
        write_document(
            &self.directory,
            &self.directory.join("iam-private-access.json"),
            records,
        )
    }
}

async fn cleanup(
    iam: &AdministrationService,
    connections: &ConnectionService,
    session: &Session,
    record: &ProvisioningRecord,
) -> Result<()> {
    let name = record.view.user_name.as_str();
    let response = call(iam, session, "GetUser", [("UserName", name)]).await;
    if let Some(response) = existing(response)? {
        let user = xml::<ReadUser>(&response)?.result.user;
        assert_identity(record, &user)?;
        remove_dependencies(iam, session, name).await?;
        existing(call(iam, session, "DeleteUser", [("UserName", name)]).await)?;
    }
    connections.remove_managed_iam(record.view.id)
}

async fn remove_dependencies(
    iam: &AdministrationService,
    session: &Session,
    name: &str,
) -> Result<()> {
    clean_collection(
        iam,
        session,
        name,
        "ListAccessKeys",
        "DeleteAccessKey",
        "AccessKeyId",
        |page| values(page.access_key_metadata, |key| key.access_key_id),
    )
    .await?;
    clean_collection(
        iam,
        session,
        name,
        "ListGroupsForUser",
        "RemoveUserFromGroup",
        "GroupName",
        |page| values(page.groups, |group| group.group_name),
    )
    .await?;
    clean_collection(
        iam,
        session,
        name,
        "ListAttachedUserPolicies",
        "DetachUserPolicy",
        "PolicyArn",
        |page| values(page.attached_policies, |policy| policy.policy_arn),
    )
    .await?;
    clean_collection(
        iam,
        session,
        name,
        "ListUserPolicies",
        "DeleteUserPolicy",
        "PolicyName",
        |page| values(page.policy_names, |name| name),
    )
    .await
}

fn values<T>(members: Option<Members<T>>, value: impl Fn(T) -> String) -> Vec<String> {
    members
        .map(|members| members.member.into_iter().map(value).collect())
        .unwrap_or_default()
}

async fn clean_collection(
    iam: &AdministrationService,
    session: &Session,
    name: &str,
    list: &str,
    remove: &str,
    field: &str,
    rows: impl Fn(CleanupPage) -> Vec<String>,
) -> Result<()> {
    let mut marker = String::new();
    let mut visited = BTreeSet::new();
    let mut items = Vec::new();
    loop {
        let response = call(
            iam,
            session,
            list,
            [("UserName", name), ("Marker", &marker)],
        )
        .await?;
        let page = xml::<CleanupResponse>(&response)?.result;
        let truncated = page.is_truncated;
        let next = page.marker.clone();
        items.extend(rows(page));
        if !truncated {
            break;
        }
        marker = next
            .filter(|value| !value.is_empty() && visited.insert(value.clone()))
            .ok_or_else(|| {
                AppError::Storage("IAM returned an invalid cleanup page marker.".into())
            })?;
    }
    for value in items {
        existing(call(iam, session, remove, [("UserName", name), (field, &value)]).await)?;
    }
    Ok(())
}

pub(super) async fn ensure_generic_iam_write(
    iam: &AdministrationService,
    session: &Session,
    action: &str,
    values: &BTreeMap<String, String>,
) -> Result<()> {
    validate_generic_iam_target(values)?;
    if matches!(
        action,
        "CreateAccessKey" | "UpdateAccessKey" | "DeleteAccessKey"
    ) && !values.contains_key("UserName")
    {
        let response = iam.iam(session, "GetUser", BTreeMap::new()).await?;
        if is_reserved(&xml::<ReadUser>(&response)?.result.user.user_name) {
            return Err(protected_identity());
        }
    }
    Ok(())
}

fn validate_generic_iam_target(values: &BTreeMap<String, String>) -> Result<()> {
    if ["UserName", "NewUserName"]
        .into_iter()
        .any(|key| values.get(key).is_some_and(|name| is_reserved(name)))
        || ["Path", "NewPath"].into_iter().any(|key| {
            values
                .get(key)
                .is_some_and(|path| path.starts_with("/storeforge-private/"))
        })
    {
        return Err(protected_identity());
    }
    Ok(())
}

fn is_reserved(name: &str) -> bool {
    name.trim()
        .to_ascii_lowercase()
        .starts_with(RESERVED_PREFIX)
}

fn validate_draft(draft: &PrivateAccessDraft) -> Result<()> {
    if draft.name.trim().is_empty() || (!draft.browser && !draft.manager) {
        return Err(AppError::Invalid(
            "Enter a connection name and enable a workspace.".into(),
        ));
    }
    for policy in &draft.inline_policies {
        if policy.name.trim().is_empty()
            || !serde_json::from_str::<serde_json::Value>(&policy.document)
                .is_ok_and(|value| value.is_object())
        {
            return Err(AppError::Invalid(
                "Enter a name and a JSON object for each inline policy.".into(),
            ));
        }
    }
    Ok(())
}

fn authority(session: &Session) -> Authority {
    Authority {
        connection: session.context.connection_id,
        endpoint: session.context.endpoint_id,
        tenant: session.context.tenant.clone(),
        account: session.context.account.clone(),
        provider: session.profile.provider,
        principal_key: session.credentials.access_key_id().into(),
    }
}

fn user_path(id: Uuid) -> String {
    format!("/storeforge-private/{id}/")
}

fn assert_identity(record: &ProvisioningRecord, user: &User) -> Result<()> {
    if !user.user_name.eq_ignore_ascii_case(&record.view.user_name)
        || user.path != user_path(record.view.id)
        || user.user_id.is_empty()
        || record
            .user_id
            .as_ref()
            .is_some_and(|id| id != &user.user_id)
    {
        return Err(AppError::Storage(
            "The provider identity does not match the provisioning record.".into(),
        ));
    }
    Ok(())
}

fn protected_identity() -> AppError {
    AppError::Invalid("Manage this identity through IAM personal access.".into())
}

fn existing(response: Result<String>) -> Result<Option<String>> {
    match response {
        Ok(response) => Ok(Some(response)),
        Err(AppError::Missing(_)) => Ok(None),
        Err(AppError::Storage(message)) if message.contains("<Code>NoSuchEntity</Code>") => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn xml<T: DeserializeOwned>(response: &str) -> Result<T> {
    quick_xml::de::from_str(response)
        .map_err(|_| AppError::Storage("The provider returned an unexpected IAM response.".into()))
}

async fn call<const N: usize>(
    iam: &AdministrationService,
    session: &Session,
    action: &str,
    fields: [(&str, &str); N],
) -> Result<String> {
    iam.iam(
        session,
        action,
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    )
    .await
}

#[cfg(test)]
#[path = "../../tests/unit/administration/private_access.rs"]
mod tests;
