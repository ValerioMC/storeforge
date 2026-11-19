use std::{collections::HashSet, path::PathBuf};

use chrono::Utc;
use reqwest::Method;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::{
        model::{IdentityKind as ConnectionIdentity, ManagedRgwOwner, Provider, Secret, Workspace},
        service::{ConnectionService, Session},
    },
};

use super::{
    model::*,
    repository::{self, Fields, RegistryRepository, add, call, user_fields},
};

const IMPORT_LIMIT: usize = 50;
const ROOT_SCAN_LIMIT: usize = 500;

pub struct RgwIdentityService {
    repository: RegistryRepository,
    updates: Mutex<()>,
}

impl RgwIdentityService {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            repository: RegistryRepository::new(directory),
            updates: Mutex::new(()),
        }
    }

    pub async fn read(
        &self,
        connections: &ConnectionService,
        connection: Uuid,
        request: ReadRequest,
    ) -> Result<ReadResponse> {
        let session = admin_session(connections, connection)?;
        let scope = scope(&session);
        let registry = self.repository.read()?;
        match request {
            ReadRequest::List {
                kind,
                managed,
                marker,
            } => inventory(
                &session,
                &scope,
                &registry,
                kind,
                managed,
                marker.as_deref(),
            )
            .await
            .map(ReadResponse::List),
            ReadRequest::Account { id } => {
                let mut account = repository::account(&session, &id).await?;
                if let Some(root) = registered_root(&registry, &scope, &account.id) {
                    account.root = Some(root.clone());
                }
                Ok(ReadResponse::Account(account))
            }
            ReadRequest::AccountUsage { id } => {
                let account = repository::account(&session, &id).await?;
                repository::account_usage(&session, &account)
                    .await
                    .map(ReadResponse::Usage)
            }
            ReadRequest::User { identity } => {
                let user = repository::user(&session, &identity, true).await?;
                Ok(ReadResponse::User(with_key_metadata(
                    user, &registry, &scope,
                )))
            }
            ReadRequest::DelegatedKeys => {
                let user = delegated_owner(connections, &session).await?;
                Ok(ReadResponse::User(with_key_metadata(
                    user, &registry, &scope,
                )))
            }
        }
    }

    pub async fn change(
        &self,
        connections: &ConnectionService,
        connection: Uuid,
        request: ChangeRequest,
    ) -> Result<ChangeResponse> {
        let _update = self.updates.lock().await;
        let session = admin_session(connections, connection)?;
        let scope = scope(&session);
        let mut registry = self.repository.read()?;
        match request {
            ChangeRequest::ImportAccounts { entries } => {
                validate_batch(entries.len())?;
                for entry in entries {
                    let account = repository::account(&session, &entry.id).await?;
                    let root = discover_root(&session, &account, entry.root).await?;
                    register(
                        &mut registry,
                        &scope,
                        IdentityTarget::Account { id: account.id },
                        Some(root),
                    )?;
                }
                self.repository.write(&registry)?;
            }
            ChangeRequest::ImportUsers { identities } => {
                validate_batch(identities.len())?;
                for identity in identities {
                    let user = repository::user(&session, &identity, false).await?;
                    assert_standalone(&user)?;
                    register(
                        &mut registry,
                        &scope,
                        IdentityTarget::User { identity },
                        None,
                    )?;
                }
                self.repository.write(&registry)?;
            }
            ChangeRequest::CreateAccount(creation) => {
                return self
                    .create_account(&session, &scope, &mut registry, creation)
                    .await;
            }
            ChangeRequest::UpdateAccount { id, draft } => {
                let mut fields = account_fields(draft)?;
                fields.push(("id", id));
                call(&session, Method::PUT, "account", fields).await?;
            }
            ChangeRequest::CreateUser {
                identity,
                draft,
                account,
                root,
                managed,
            } => {
                let mut response = ChangeResponse::default();
                validate_new_user(&session, &identity, account.as_deref(), root, managed).await?;
                create_user(&session, &identity, draft, account.as_deref(), root).await?;
                let target = IdentityTarget::User { identity };
                if managed {
                    register(&mut registry, &scope, target.clone(), None)?;
                    if self.repository.write(&registry).is_err() {
                        response.issue = Some("The RGW user exists, but Storeforge could not register it. Import it from the cluster inventory.".into());
                    }
                }
                response.target = Some(target);
                return Ok(response);
            }
            ChangeRequest::UpdateUser { identity, draft } => {
                let mut fields = user_fields(&identity)?;
                fields.extend(user_draft_fields(draft));
                call(&session, Method::POST, "user", fields).await?;
            }
            ChangeRequest::SetQuota {
                target,
                bucket,
                quota,
            } => {
                set_quota(&session, target, bucket, quota).await?;
            }
            ChangeRequest::Key {
                scope: key_scope,
                owner,
                change,
            } => {
                let user = match key_scope {
                    KeyScope::Delegated => delegated_owner(connections, &session).await?,
                    KeyScope::Inventory => repository::user(&session, &owner, false).await?,
                };
                if user.identity != owner {
                    return Err(AppError::Invalid(
                        "The selected connection no longer owns this RGW identity.".into(),
                    ));
                }
                return self
                    .change_key(connections, &session, &scope, &mut registry, user, change)
                    .await;
            }
        }
        Ok(ChangeResponse::default())
    }

    async fn create_account(
        &self,
        session: &Session,
        scope: &RegistryScope,
        registry: &mut Registry,
        creation: AccountCreation,
    ) -> Result<ChangeResponse> {
        let AccountCreation {
            id,
            tenant,
            draft,
            root_uid,
            managed,
        } = creation;
        let name = draft.name.clone();
        let root = root_uid
            .map(|uid| repository::parse_identity(&uid, &tenant))
            .transpose()?;
        if managed && root.is_none() {
            return Err(AppError::Invalid(
                "Choose a root UID for a managed RGW account.".into(),
            ));
        }
        if let Some(root) = &root {
            if root.tenant != tenant {
                return Err(AppError::Invalid(
                    "The root UID must belong to the account tenant.".into(),
                ));
            }
            ensure_new_uid(session, root).await?;
        }
        let mut fields = account_fields(draft)?;
        add(&mut fields, "id", id.as_deref());
        fields.push(("tenant", tenant.clone()));
        let document = call(session, Method::POST, "account", fields).await?;
        let account = repository::created_account(&document).map_err(|_| AppError::AmbiguousWrite(
            "RGW accepted account creation but did not verify its ID. Refresh the cluster inventory before retrying.".into()
        ))?;
        if id.as_ref().is_some_and(|id| id != &account.id) || tenant != account.tenant {
            return Err(AppError::AmbiguousWrite("RGW returned a different account identity after creation. Review the cluster inventory.".into()));
        }
        let target = IdentityTarget::Account {
            id: account.id.clone(),
        };
        let mut response = ChangeResponse {
            target: Some(target.clone()),
            ..ChangeResponse::default()
        };
        if let Some(root) = &root {
            let result = create_account_root(session, &account, root, &name).await;
            if let Err(error) = result {
                response.issue = Some(format!(
                    "Account {} exists. Root setup requires attention: {error}",
                    account.id
                ));
                return Ok(response);
            }
        }
        if managed {
            register(registry, scope, target, root)?;
            if self.repository.write(registry).is_err() {
                response.issue = Some(format!(
                    "Account {} exists with its root. Import it to finish registration.",
                    account.id
                ));
            }
        }
        Ok(response)
    }

    async fn change_key(
        &self,
        connections: &ConnectionService,
        session: &Session,
        scope: &RegistryScope,
        registry: &mut Registry,
        user: UserDetail,
        change: KeyChange,
    ) -> Result<ChangeResponse> {
        let owner = &user.identity;
        let keys = user.keys.as_ref().ok_or_else(|| AppError::Invalid(
            "RGW did not return key inventory for this user. Check that the Admin Ops identity can read user keys.".into()
        ))?;
        match change {
            KeyChange::Create { metadata, private } => {
                validate_metadata(&metadata)?;
                validate_expiration(&metadata)?;
                if private && metadata.name.trim().is_empty() {
                    return Err(AppError::Invalid(
                        "Enter a name for the private connection.".into(),
                    ));
                }
                if user.suspended == Some(true) {
                    return Err(AppError::Invalid(
                        "Resume this user before creating a key.".into(),
                    ));
                }
                let mut fields = user_fields(owner)?;
                fields.extend([
                    ("key", "true".into()),
                    ("key-type", "s3".into()),
                    ("generate-key", "true".into()),
                ]);
                let document = call(session, Method::PUT, "user", fields).await?;
                let previous = keys.iter().map(|key| key.id.clone()).collect();
                let issued = repository::issued_key(&document, owner, &previous)?;
                let issuance = KeyIssuance {
                    issued,
                    metadata,
                    private,
                };
                return self.finish_key_creation(connections, session, registry, &user, issuance);
            }
            KeyChange::Metadata { id, metadata } => {
                validate_metadata(&metadata)?;
                let key = selected_key(keys, &id)?;
                let record = key_record(registry, scope, owner, &id, session.profile.id);
                if record.metadata.expires_at != metadata.expires_at {
                    reject_connection_key(&user, &id)?;
                    validate_expiration(&metadata)?;
                }
                if record
                    .enforcement
                    .as_ref()
                    .is_some_and(|state| !matches!(state, Enforcement::Pending))
                    && metadata.expires_at != record.metadata.expires_at
                    && key.active != Some(false)
                {
                    return Err(AppError::Invalid(
                        "Confirm the key is inactive before replacing its expired schedule.".into(),
                    ));
                }
                if record.metadata.expires_at != metadata.expires_at {
                    record.enforcement = metadata.expires_at.map(|_| Enforcement::Pending);
                }
                record.metadata = metadata;
                record.administrator = session.profile.id;
                self.repository.write(registry)?;
            }
            KeyChange::Status { id, active } => {
                selected_key(keys, &id)?;
                reject_connection_key(&user, &id)?;
                if active
                    && registry.keys.iter().any(|key| {
                        key.scope == *scope
                            && key.owner == *owner
                            && key.id == id
                            && key
                                .metadata
                                .expires_at
                                .is_some_and(|deadline| deadline <= Utc::now())
                    })
                {
                    return Err(AppError::Invalid("Set a future expiration or clear the expired schedule before activating this key.".into()));
                }
                set_key_status(session, owner, &id, active).await?;
            }
            KeyChange::Delete { id } => {
                reject_connection_key(&user, &id)?;
                let mut response = ChangeResponse::default();
                let record_position = registry
                    .keys
                    .iter()
                    .position(|key| key.scope == *scope && key.owner == *owner && key.id == id);
                if keys.iter().any(|key| key.id == id) {
                    let mut fields = user_fields(owner)?;
                    fields.extend([
                        ("key", id.clone()),
                        ("key-type", "s3".into()),
                        ("access-key", id),
                    ]);
                    call(session, Method::DELETE, "user", fields).await?;
                } else if record_position
                    .is_none_or(|position| registry.keys[position].private_profile.is_none())
                {
                    return Err(AppError::Missing(
                        "This key is no longer listed under the selected RGW owner.".into(),
                    ));
                }
                if let Some(position) = record_position {
                    if let Some(profile) = registry.keys[position].private_profile
                        && let Err(error) = connections.remove_managed_rgw(profile)
                    {
                        registry.keys[position].issue = Some(format!(
                            "RGW key deleted. Local profile cleanup failed: {error}"
                        ));
                        response.issue = registry.keys[position].issue.clone();
                        self.repository.write(registry)?;
                        return Ok(response);
                    }
                    registry.keys.remove(position);
                    self.repository.write(registry)?;
                }
                return Ok(response);
            }
        }
        Ok(ChangeResponse::default())
    }

    fn finish_key_creation(
        &self,
        connections: &ConnectionService,
        session: &Session,
        registry: &mut Registry,
        user: &UserDetail,
        issuance: KeyIssuance,
    ) -> Result<ChangeResponse> {
        let KeyIssuance {
            issued,
            metadata,
            private,
        } = issuance;
        let scope = scope(session);
        let record = key_record(
            registry,
            &scope,
            &user.identity,
            &issued.access_key,
            session.profile.id,
        );
        record.enforcement = metadata.expires_at.map(|_| Enforcement::Pending);
        record.metadata = metadata;
        let name = record.metadata.name.trim().to_owned();
        let mut response = ChangeResponse::default();
        if private {
            let profile_id = Uuid::new_v4();
            record.private_profile = Some(profile_id);
            record.issue = Some("Private connection setup requires reconciliation.".into());
        }
        if self.repository.write(registry).is_err() {
            response.issue = Some("RGW issued this key, but Storeforge could not save its metadata or provision the private connection. The expiration schedule is not active.".into());
            response.issued = Some(issued);
            return Ok(response);
        }
        if private {
            let record = key_record(
                registry,
                &scope,
                &user.identity,
                &issued.access_key,
                session.profile.id,
            );
            let id = record.private_profile.ok_or_else(|| {
                AppError::File("The private profile record is incomplete.".into())
            })?;
            let owner = ManagedRgwOwner {
                administrator: session.profile.id,
                tenant: user.identity.tenant.clone(),
                uid: user.identity.uid.clone(),
                account: user.account.clone().unwrap_or_default(),
                access_key_id: issued.access_key.clone(),
            };
            let secret = Secret {
                access_key: issued.access_key.clone(),
                secret_key: issued.secret_key.clone(),
                session_token: None,
            };
            match connections.save_managed_rgw(session, id, name, owner, secret) {
                Ok(profile) => {
                    record.issue = None;
                    response.profile = Some(profile);
                }
                Err(error) => {
                    record.private_profile = None;
                    record.issue = Some(format!(
                        "RGW key exists, but the private profile could not be stored: {error}"
                    ));
                    response.issue = record.issue.clone();
                    response.issued = Some(issued);
                }
            }
            if self.repository.write(registry).is_err() {
                response.issue = Some("RGW issued the key. Storeforge could not update the local provisioning receipt. Refresh the inventory and Connections.".into());
            }
        } else {
            response.issued = Some(issued);
        }
        Ok(response)
    }

    pub async fn enforce_expirations(&self, connections: &ConnectionService) -> Result<()> {
        let scheduled = self.repository.read()?;
        for expired in scheduled.keys.iter().filter(|key| expiration_due(key)) {
            self.enforce_one(connections, expired).await?;
        }
        Ok(())
    }

    async fn enforce_one(
        &self,
        connections: &ConnectionService,
        expired: &KeyRecord,
    ) -> Result<()> {
        let _update = self.updates.lock().await;
        let mut registry = self.repository.read()?;
        let Some(key) = registry.keys.iter_mut().find(|key| {
            key.scope == expired.scope
                && key.owner == expired.owner
                && key.id == expired.id
                && expiration_due(key)
        }) else {
            return Ok(());
        };
        key.enforcement = Some(match expire_key(connections, key).await {
            Ok(()) => Enforcement::Disabled,
            Err(error) => {
                tracing::warn!(connection = %key.administrator, %error, "RGW key expiration enforcement failed");
                Enforcement::Failed {
                    message: error.to_string(),
                }
            }
        });
        self.repository.write(&registry)
    }
}

fn expiration_due(key: &KeyRecord) -> bool {
    key.metadata
        .expires_at
        .is_some_and(|deadline| deadline <= Utc::now())
        && !matches!(key.enforcement, Some(Enforcement::Disabled))
}

struct KeyIssuance {
    issued: IssuedKey,
    metadata: KeyMetadata,
    private: bool,
}

fn admin_session(connections: &ConnectionService, id: Uuid) -> Result<Session> {
    let session =
        connections.session_for(id, Workspace::CephAdmin, ConnectionIdentity::AdminOps)?;
    if session.profile.provider != Provider::Ceph {
        return Err(AppError::Invalid(
            "Select a Ceph RGW connection with verified Admin Ops permissions.".into(),
        ));
    }
    Ok(session)
}

fn scope(session: &Session) -> RegistryScope {
    RegistryScope {
        endpoint: session.context.endpoint_id,
        admin_url: session.profile.endpoint.trim_end_matches('/').into(),
    }
}

async fn delegated_owner(connections: &ConnectionService, session: &Session) -> Result<UserDetail> {
    let s3 = connections.session_for(
        session.profile.id,
        Workspace::Manager,
        ConnectionIdentity::S3,
    )?;
    if s3.credentials.session_token().is_some() {
        return Err(AppError::Invalid(
            "Delegated RGW keys require a saved permanent S3 credential.".into(),
        ));
    }
    let user = repository::owner(session, s3.credentials.access_key_id()).await?;
    if user.identity.tenant != s3.context.tenant
        || user.account.as_deref().unwrap_or_default() != s3.context.account
    {
        return Err(AppError::Invalid("The RGW key owner does not match the connection's tenant and account. Verify the saved scope.".into()));
    }
    Ok(user)
}

async fn inventory(
    session: &Session,
    scope: &RegistryScope,
    registry: &Registry,
    kind: IdentityKind,
    managed: bool,
    marker: Option<&str>,
) -> Result<InventoryPage> {
    let registered = registry
        .identities
        .iter()
        .filter(|entry| entry.scope == *scope);
    if managed {
        let rows = registered
            .filter(|entry| {
                matches!(
                    (&entry.target, kind),
                    (IdentityTarget::Account { .. }, IdentityKind::Account)
                        | (IdentityTarget::User { .. }, IdentityKind::User)
                )
            })
            .map(|entry| InventoryRow {
                target: entry.target.clone(),
                managed: true,
                root: entry.root.clone(),
            })
            .collect();
        return repository::registered_page(rows, marker);
    }
    let (rows, marker) = repository::list(session, kind, marker).await?;
    let mut seen = HashSet::new();
    let items = rows
        .into_iter()
        .filter(|target| seen.insert(target.clone()))
        .map(|target| {
            let entry = registry
                .identities
                .iter()
                .find(|entry| entry.scope == *scope && (entry.target == target ||
                    matches!(&target, IdentityTarget::User { identity } if entry.root.as_ref() == Some(identity))));
            let root = if matches!(target, IdentityTarget::Account { .. }) {
                entry.and_then(|entry| entry.root.clone())
            } else {
                None
            };
            InventoryRow {
                target,
                managed: entry.is_some(),
                root,
            }
        })
        .collect();
    Ok(InventoryPage { items, marker })
}

fn validate_batch(length: usize) -> Result<()> {
    if (1..=IMPORT_LIMIT).contains(&length) {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "Select between 1 and {IMPORT_LIMIT} identities to import."
        )))
    }
}

fn register(
    registry: &mut Registry,
    scope: &RegistryScope,
    target: IdentityTarget,
    root: Option<UserIdentity>,
) -> Result<()> {
    let root_target = root.as_ref().map(|identity| IdentityTarget::User {
        identity: identity.clone(),
    });
    let conflicts = registry.identities.iter().any(|entry| {
        entry.scope == *scope
            && (entry.target == target
                || root_target.as_ref() == Some(&entry.target)
                || match &target {
                    IdentityTarget::User { identity } => entry.root.as_ref() == Some(identity),
                    IdentityTarget::Account { .. } => root.is_some() && entry.root == root,
                })
    });
    if conflicts {
        return Err(AppError::Invalid(
            "An RGW identity in this import already has a managed owner on this endpoint.".into(),
        ));
    }
    registry.identities.push(Registration {
        scope: scope.clone(),
        target,
        root,
    });
    Ok(())
}

fn registered_root<'a>(
    registry: &'a Registry,
    scope: &RegistryScope,
    id: &str,
) -> Option<&'a UserIdentity> {
    registry
        .identities
        .iter()
        .find_map(|entry| match &entry.target {
            IdentityTarget::Account { id: account } if account == id && entry.scope == *scope => {
                entry.root.as_ref()
            }
            _ => None,
        })
}

async fn discover_root(
    session: &Session,
    account: &AccountDetail,
    explicit: Option<UserIdentity>,
) -> Result<UserIdentity> {
    if let Some(mut identity) = explicit {
        if identity.tenant.is_empty() {
            identity.tenant.clone_from(&account.tenant);
        }
        let user = repository::user(session, &identity, false).await?;
        assert_root(account, &user)?;
        return Ok(identity);
    }
    let identities = match &account.users {
        Some(users) if users.len() <= ROOT_SCAN_LIMIT => users.clone(),
        Some(_) => return Err(root_search_limit()),
        None => root_candidates(session, &account.tenant).await?,
    };
    let mut root: Option<UserIdentity> = None;
    let mut verified = HashSet::new();
    for identity in identities
        .into_iter()
        .filter(|identity| identity.tenant == account.tenant)
    {
        if !verified.insert(identity.clone()) {
            continue;
        }
        let user = match repository::user(session, &identity, false).await {
            Ok(user) => user,
            Err(AppError::Missing(_)) => continue,
            Err(error) => return Err(error),
        };
        if user.account.as_deref() == Some(account.id.as_str()) && user.account_root == Some(true) {
            if root.is_some() {
                return Err(AppError::Invalid(
                    "RGW reports multiple account roots. Select the intended root UID explicitly."
                        .into(),
                ));
            }
            root = Some(identity);
        }
    }
    root.ok_or_else(|| AppError::Invalid("No RGW root was verified for this account. Select its exact root UID or create it in Ceph Admin.".into()))
}

async fn root_candidates(session: &Session, tenant: &str) -> Result<Vec<UserIdentity>> {
    let mut identities = Vec::new();
    let mut marker = None;
    let mut seen = HashSet::new();
    let mut scanned = 0;
    loop {
        let (rows, next) = repository::list(session, IdentityKind::User, marker.as_deref()).await?;
        scanned += rows.len();
        if scanned > ROOT_SCAN_LIMIT || seen.len() >= ROOT_SCAN_LIMIT {
            return Err(root_search_limit());
        }
        identities.extend(rows.into_iter().filter_map(|row| match row {
            IdentityTarget::User { identity } if identity.tenant == tenant => Some(identity),
            _ => None,
        }));
        let Some(next) = next else {
            return Ok(identities);
        };
        if !seen.insert(next.clone()) {
            return Err(AppError::Storage(
                "RGW repeated a root discovery page marker.".into(),
            ));
        }
        marker = Some(next);
    }
}

fn root_search_limit() -> AppError {
    AppError::Invalid(
        "Root discovery reached its scan limit. Select the account's exact root UID.".into(),
    )
}

fn assert_root(account: &AccountDetail, user: &UserDetail) -> Result<()> {
    if user.identity.tenant == account.tenant
        && user.account.as_deref() == Some(&account.id)
        && user.account_root == Some(true)
    {
        Ok(())
    } else {
        Err(AppError::Invalid(
            "The selected user is not a verified root of this RGW account and tenant.".into(),
        ))
    }
}

fn assert_standalone(user: &UserDetail) -> Result<()> {
    if user.account.is_some() || user.account_root == Some(true) {
        return Err(AppError::Invalid(format!(
            "{} is owned by an RGW account. Manage it through that account.",
            user.identity.qualified()
        )));
    }
    Ok(())
}

async fn ensure_new_uid(session: &Session, identity: &UserIdentity) -> Result<()> {
    match repository::user(session, identity, false).await {
        Err(AppError::Missing(_)) => Ok(()),
        Ok(_) => Err(AppError::Invalid(format!(
            "{} already exists on this endpoint. Import it instead.",
            identity.qualified()
        ))),
        Err(error) => Err(error),
    }
}

async fn validate_new_user(
    session: &Session,
    identity: &UserIdentity,
    account: Option<&str>,
    root: bool,
    managed: bool,
) -> Result<()> {
    if let Some(account) = account {
        if managed {
            return Err(AppError::Invalid(
                "Register the owning account for account-owned users.".into(),
            ));
        }
        if repository::account(session, account).await?.tenant != identity.tenant {
            return Err(AppError::Invalid(
                "The RGW user and account must belong to the same tenant.".into(),
            ));
        }
    } else if root {
        return Err(AppError::Invalid(
            "Select an account before creating its root.".into(),
        ));
    }
    ensure_new_uid(session, identity).await
}

async fn create_account_root(
    session: &Session,
    account: &AccountDetail,
    identity: &UserIdentity,
    name: &str,
) -> Result<()> {
    if account.tenant != identity.tenant {
        return Err(AppError::Invalid(
            "RGW created the account in a different tenant.".into(),
        ));
    }
    let display_name: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || "_.-".contains(character) {
                character
            } else {
                '-'
            }
        })
        .collect();
    let draft = UserDraft {
        display_name: Some(display_name),
        email: None,
        suspended: None,
        max_buckets: None,
    };
    let user = create_user(session, identity, draft, Some(&account.id), true).await?;
    assert_root(account, &user)
}

async fn create_user(
    session: &Session,
    identity: &UserIdentity,
    draft: UserDraft,
    account: Option<&str>,
    root: bool,
) -> Result<UserDetail> {
    if draft
        .display_name
        .as_ref()
        .is_none_or(|name| name.trim().is_empty())
    {
        return Err(AppError::Invalid("Enter an RGW user display name.".into()));
    }
    let mut fields = user_fields(identity)?;
    fields.extend(user_draft_fields(draft));
    fields.push(("generate-key", "false".into()));
    add(&mut fields, "account-id", account);
    if root {
        fields.push(("account-root", "true".into()));
    }
    call(session, Method::PUT, "user", fields).await?;
    let user = repository::user(session, identity, false).await.map_err(|error| AppError::AmbiguousWrite(
        format!("RGW accepted user creation but ownership could not be read back. Refresh before retrying. {error}")))?;
    if user.account.as_deref() != account || (root && user.account_root != Some(true)) {
        return Err(AppError::AmbiguousWrite("RGW created the user but did not verify the requested ownership. Review the cluster record.".into()));
    }
    Ok(user)
}

fn account_fields(draft: AccountDraft) -> Result<Fields> {
    if draft.name.trim().is_empty() {
        return Err(AppError::Invalid("Enter an RGW account name.".into()));
    }
    let mut fields = vec![("name", draft.name)];
    add(&mut fields, "email", draft.email);
    add(&mut fields, "max-users", draft.limits.max_users);
    add(&mut fields, "max-buckets", draft.limits.max_buckets);
    add(&mut fields, "max-roles", draft.limits.max_roles);
    add(&mut fields, "max-groups", draft.limits.max_groups);
    add(&mut fields, "max-access-keys", draft.limits.max_access_keys);
    Ok(fields)
}

fn user_draft_fields(draft: UserDraft) -> Fields {
    let mut fields = Vec::new();
    add(&mut fields, "display-name", draft.display_name);
    add(&mut fields, "email", draft.email);
    add(&mut fields, "suspended", draft.suspended);
    add(&mut fields, "max-buckets", draft.max_buckets);
    fields
}

async fn set_quota(
    session: &Session,
    target: IdentityTarget,
    bucket: bool,
    quota: Quota,
) -> Result<()> {
    if [quota.max_bytes, quota.max_objects]
        .into_iter()
        .flatten()
        .any(|limit| limit < -1)
    {
        return Err(AppError::Invalid(
            "Quota limits must be nonnegative or -1 for unlimited.".into(),
        ));
    }
    let (resource, mut fields) = match target {
        IdentityTarget::Account { id } => ("account", vec![("id", id)]),
        IdentityTarget::User { identity } => ("user", user_fields(&identity)?),
    };
    fields.extend([
        ("quota", String::new()),
        (
            "quota-type",
            if bucket {
                "bucket".into()
            } else {
                resource.into()
            },
        ),
        ("enabled", quota.enabled.to_string()),
    ]);
    add(&mut fields, "max-size", quota.max_bytes);
    add(&mut fields, "max-objects", quota.max_objects);
    call(session, Method::PUT, resource, fields).await?;
    Ok(())
}

fn with_key_metadata(
    mut user: UserDetail,
    registry: &Registry,
    scope: &RegistryScope,
) -> UserDetail {
    if let Some(keys) = &mut user.keys {
        for key in keys.iter_mut() {
            if let Some(record) = registry.keys.iter().find(|record| {
                record.scope == *scope && record.owner == user.identity && record.id == key.id
            }) {
                key.metadata = record.metadata.clone();
                key.enforcement = record.enforcement.clone();
                key.private_profile = record.private_profile;
                key.issue = record.issue.clone();
            }
        }
        for record in registry.keys.iter().filter(|record| {
            record.scope == *scope
                && record.owner == user.identity
                && record.private_profile.is_some()
        }) {
            if keys.iter().any(|key| key.id == record.id) {
                continue;
            }
            keys.push(AccessKey {
                id: record.id.clone(),
                active: None,
                metadata: record.metadata.clone(),
                enforcement: record.enforcement.clone(),
                private_profile: record.private_profile,
                issue: record.issue.clone(),
                provider_missing: true,
            });
        }
    }
    user
}

fn key_record<'a>(
    registry: &'a mut Registry,
    scope: &RegistryScope,
    owner: &UserIdentity,
    id: &str,
    administrator: Uuid,
) -> &'a mut KeyRecord {
    if let Some(position) = registry
        .keys
        .iter()
        .position(|key| key.scope == *scope && key.owner == *owner && key.id == id)
    {
        return &mut registry.keys[position];
    }
    registry.keys.push(KeyRecord {
        scope: scope.clone(),
        owner: owner.clone(),
        id: id.into(),
        administrator,
        metadata: KeyMetadata::default(),
        enforcement: None,
        private_profile: None,
        issue: None,
    });
    registry
        .keys
        .last_mut()
        .expect("the inserted key record exists")
}

fn validate_metadata(metadata: &KeyMetadata) -> Result<()> {
    if metadata.name.len() > 150 || metadata.notes.len() > 2000 {
        return Err(AppError::Invalid(
            "Limit key names to 150 bytes and notes to 2,000 bytes.".into(),
        ));
    }
    Ok(())
}

fn validate_expiration(metadata: &KeyMetadata) -> Result<()> {
    if metadata
        .expires_at
        .is_some_and(|deadline| deadline <= Utc::now())
    {
        return Err(AppError::Invalid(
            "Choose an expiration in the future.".into(),
        ));
    }
    Ok(())
}

fn reject_connection_key(user: &UserDetail, id: &str) -> Result<()> {
    if user.connection_key_id.as_deref() == Some(id) {
        return Err(AppError::Invalid("Use a separate administrator connection to change the credential used by this connection.".into()));
    }
    Ok(())
}

fn selected_key<'a>(keys: &'a [AccessKey], id: &str) -> Result<&'a AccessKey> {
    keys.iter().find(|key| key.id == id).ok_or_else(|| {
        AppError::Missing("This key is no longer listed under the selected RGW owner.".into())
    })
}

async fn set_key_status(
    session: &Session,
    owner: &UserIdentity,
    id: &str,
    active: bool,
) -> Result<()> {
    let mut fields = user_fields(owner)?;
    fields.extend([
        ("key", "true".into()),
        ("key-type", "s3".into()),
        ("generate-key", "false".into()),
        ("access-key", id.into()),
        ("active", active.to_string()),
    ]);
    call(session, Method::PUT, "user", fields).await?;
    let current = repository::user(session, owner, false).await?;
    let verified = current
        .keys
        .as_ref()
        .and_then(|keys| keys.iter().find(|key| key.id == id))
        .and_then(|key| key.active);
    if verified != Some(active) {
        return Err(AppError::AmbiguousWrite("The gateway did not confirm the requested key state. Check whether this RGW version supports activation and deactivation.".into()));
    }
    Ok(())
}

async fn expire_key(connections: &ConnectionService, key: &KeyRecord) -> Result<()> {
    let session = admin_session(connections, key.administrator)?;
    if scope(&session) != key.scope {
        return Err(AppError::Invalid(
            "The expiration profile now points to a different RGW endpoint.".into(),
        ));
    }
    let user = match repository::user(&session, &key.owner, false).await {
        Ok(user) => user,
        Err(AppError::Missing(_)) => return Ok(()),
        Err(error) => return Err(error),
    };
    let keys = user
        .keys
        .ok_or_else(|| AppError::Invalid("RGW did not report the owner's keys.".into()))?;
    match keys.iter().find(|candidate| candidate.id == key.id) {
        None
        | Some(AccessKey {
            active: Some(false),
            ..
        }) => Ok(()),
        Some(_) => set_key_status(&session, &key.owner, &key.id, false).await,
    }
}
