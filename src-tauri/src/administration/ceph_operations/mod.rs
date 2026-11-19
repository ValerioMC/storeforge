use std::collections::HashSet;

use reqwest::Method;
use uuid::Uuid;

use crate::{
    bucket::workbench::{BucketTarget, InventorySource, validate_revision},
    common::error::{AppError, Result},
    connection::{
        model::{CapabilityState, IdentityKind, Workspace},
        service::{ConnectionService, Session},
    },
};

use super::repository::RgwClient;
use super::rgw_identities::{
    model::{IdentityTarget, UserIdentity},
    repository as identities,
};
pub(crate) use model::{Action, BucketChange, BucketOwner, IndexOutcome, Outcome};
use repository::{Fields, bucket_binding, bucket_label, call, rejected};

mod model;
mod repository;

impl Action {
    pub fn identity(&self) -> IdentityKind {
        match self {
            Self::Bucket { .. } => IdentityKind::CephAdmin,
            Self::DeleteUser { .. } | Self::DeleteAccount { .. } => IdentityKind::AdminOps,
        }
    }

    pub fn audit_target(&self) -> Option<(&'static str, String)> {
        match self {
            Self::Bucket { target, change, .. } => {
                let label = match change {
                    BucketChange::Delete { .. } => "Delete RGW bucket",
                    BucketChange::Link { .. } => "Link RGW bucket",
                    BucketChange::Unlink => "Unlink RGW bucket",
                    BucketChange::Index { repair: true, .. } => "Repair RGW bucket index",
                    BucketChange::Index { repair: false, .. } | BucketChange::Browse => {
                        return None;
                    }
                };
                Some((label, bucket_label(target)))
            }
            Self::DeleteUser { identity, .. } => Some(("Delete RGW user", identity.qualified())),
            Self::DeleteAccount { id, .. } => Some(("Delete RGW account", id.clone())),
        }
    }
}

pub(crate) async fn execute(
    connections: &ConnectionService,
    connection: Uuid,
    revision: Option<Uuid>,
    action: Action,
) -> Result<Outcome> {
    let session = connections.session_for(connection, Workspace::CephAdmin, action.identity())?;
    validate_revision(&session, revision)?;
    match action {
        Action::Bucket {
            target,
            owner,
            change,
            confirmation,
        } => {
            change_bucket(
                &session,
                &RgwClient::new(&session)?,
                &target,
                owner.as_deref(),
                change,
                &confirmation,
            )
            .await
        }
        Action::DeleteUser {
            identity,
            account,
            purge_data,
            confirmation,
        } => {
            let protected = ceph_principal(connections, &session)?;
            delete_user(
                &session,
                protected.as_ref(),
                &identity,
                account.as_deref(),
                purge_data,
                &confirmation,
            )
            .await
        }
        Action::DeleteAccount {
            id,
            tenant,
            confirmation,
        } => {
            let protected = ceph_principal(connections, &session)?;
            delete_account(&session, protected.as_ref(), &id, &tenant, &confirmation).await
        }
    }
}

fn ceph_principal(connections: &ConnectionService, session: &Session) -> Result<Option<Session>> {
    if session.profile.capability(IdentityKind::CephAdmin) != CapabilityState::Detected {
        return Ok(None);
    }
    let protected = connections.session_for(
        session.profile.id,
        Workspace::CephAdmin,
        IdentityKind::CephAdmin,
    )?;
    validate_revision(&protected, session.profile.revision)?;
    Ok(Some(protected))
}

async fn delete_user(
    session: &Session,
    protected: Option<&Session>,
    identity: &UserIdentity,
    account: Option<&str>,
    purge_data: bool,
    confirmation: &str,
) -> Result<Outcome> {
    let mut fields = identities::user_fields(identity)?;
    let verb = if purge_data { "PURGE" } else { "DELETE" };
    confirm(
        confirmation,
        format!("{verb} USER {}", identity.qualified()),
    )?;
    verify_user_account(session, identity, account).await?;
    for administrator in [Some(session), protected].into_iter().flatten() {
        let owner =
            identities::owner(administrator, administrator.credentials.access_key_id()).await?;
        if owner.identity == *identity {
            return Err(protected_principal());
        }
    }
    enable(&mut fields, "purge-data", purge_data);
    call(&RgwClient::new(session)?, Method::DELETE, "user", fields).await
}

async fn delete_account(
    session: &Session,
    protected: Option<&Session>,
    id: &str,
    tenant: &str,
    confirmation: &str,
) -> Result<Outcome> {
    confirm(confirmation, format!("DELETE ACCOUNT {id}"))?;
    if id.is_empty() || identities::account(session, id).await?.tenant != tenant {
        return Err(AppError::Invalid(
            "The account tenant changed. Refresh the inventory.".into(),
        ));
    }
    for administrator in [Some(session), protected].into_iter().flatten() {
        let owner =
            identities::owner(administrator, administrator.credentials.access_key_id()).await?;
        if owner.account.as_deref() == Some(id) {
            return Err(protected_principal());
        }
    }
    call(
        &RgwClient::new(session)?,
        Method::DELETE,
        "account",
        vec![("id", id.into())],
    )
    .await
}

fn protected_principal() -> AppError {
    AppError::Invalid("This resource contains the active administrative principal.".into())
}

async fn verify_user_account(
    session: &Session,
    identity: &UserIdentity,
    account: Option<&str>,
) -> Result<()> {
    if identities::user(session, identity, false)
        .await?
        .account
        .as_deref()
        != account
    {
        return Err(AppError::Invalid(
            "The user’s account binding changed. Refresh the resource.".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn resolve_owner(
    session: &Session,
    target: IdentityTarget,
) -> Result<BucketOwner> {
    match target {
        IdentityTarget::Account { id } => {
            let account = identities::account(session, &id).await?;
            Ok(BucketOwner::Account {
                id: account.id,
                tenant: account.tenant,
            })
        }
        IdentityTarget::User { identity } => {
            let user = identities::user(session, &identity, false).await?;
            Ok(BucketOwner::User {
                identity: user.identity,
                account: user.account,
            })
        }
    }
}

async fn change_bucket(
    session: &Session,
    client: &RgwClient<'_>,
    target: &BucketTarget,
    owner: Option<&str>,
    change: BucketChange,
    confirmation: &str,
) -> Result<Outcome> {
    let label = bucket_label(target);
    let mut fields = vec![("bucket", label.clone())];
    let operation = bucket_method(&label, target, owner, &change, confirmation, &mut fields)?;
    match &change {
        BucketChange::Link {
            destination: BucketOwner::Account { id, tenant },
        } => {
            if identities::account(session, id).await?.tenant != *tenant {
                return Err(AppError::Invalid(
                    "The destination account tenant changed. Select the owner again.".into(),
                ));
            }
        }
        BucketChange::Link {
            destination: BucketOwner::User { identity, account },
        } => {
            verify_user_account(session, identity, account.as_deref()).await?;
        }
        _ => {}
    }
    let verify_owner =
        operation.is_some() && !matches!(change, BucketChange::Index { repair: false, .. });
    verify_bucket(session, client, target, owner, verify_owner).await?;
    match operation {
        Some(method) => call(client, method, "bucket", fields).await,
        None => authorize_browser(session, target),
    }
}

fn bucket_method(
    label: &str,
    target: &BucketTarget,
    owner: Option<&str>,
    change: &BucketChange,
    confirmation: &str,
    fields: &mut Fields,
) -> Result<Option<Method>> {
    let method = match change {
        BucketChange::Delete {
            purge_objects,
            bypass_gc,
        } => {
            if *bypass_gc && !*purge_objects {
                return Err(AppError::Invalid(
                    "Bypassing garbage collection requires an object purge.".into(),
                ));
            }
            let phrase = match (*purge_objects, *bypass_gc) {
                (true, true) => format!("PURGE BUCKET {label} BYPASS GC"),
                (true, false) => format!("PURGE BUCKET {label}"),
                _ => format!("DELETE BUCKET {label}"),
            };
            confirm(confirmation, phrase)?;
            enable(fields, "purge-objects", *purge_objects);
            enable(fields, "bypass-gc", *bypass_gc);
            Method::DELETE
        }
        BucketChange::Link { destination } => {
            confirm(
                confirmation,
                format!("LINK BUCKET {label} TO {}", destination.label()),
            )?;
            match destination {
                BucketOwner::Account { id, .. } => fields.push(("account-id", id.clone())),
                BucketOwner::User { identity, .. } => fields.extend(
                    identities::user_fields(identity)?
                        .into_iter()
                        .filter(|(field, _)| *field == "uid"),
                ),
            }
            if target.tenant.is_empty() {
                fields[0].1 = format!("/{label}");
            }
            fields.push(("bucket-id", target.instance.clone().unwrap_or_default()));
            Method::PUT
        }
        BucketChange::Unlink => {
            confirm(confirmation, format!("UNLINK BUCKET {label}"))?;
            let owner = owner.filter(|owner| !owner.is_empty()).ok_or_else(|| {
                AppError::Invalid("RGW did not identify the bucket owner.".into())
            })?;
            fields.push(owner_parameter(owner, &target.tenant));
            Method::POST
        }
        BucketChange::Index {
            repair,
            check_objects,
        } => {
            if *check_objects && !*repair {
                return Err(AppError::Invalid(
                    "Checking object state requires index repair.".into(),
                ));
            }
            if *repair {
                let suffix = if *check_objects { " WITH OBJECTS" } else { "" };
                confirm(confirmation, format!("REPAIR INDEX {label}{suffix}"))?;
            }
            fields.push(("index", String::new()));
            enable(fields, "fix", *repair);
            enable(fields, "check-objects", *check_objects);
            Method::GET
        }
        BucketChange::Browse => return Ok(None),
    };
    Ok(Some(method))
}

fn owner_parameter(owner: &str, tenant: &str) -> (&'static str, String) {
    if owner.len() == 20
        && owner
            .strip_prefix("RGW")
            .is_some_and(|suffix| suffix.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return ("account-id", owner.into());
    }
    if !tenant.is_empty() && !owner.contains('$') {
        return ("uid", format!("{tenant}${owner}"));
    }
    ("uid", owner.into())
}

async fn verify_bucket(
    session: &Session,
    client: &RgwClient<'_>,
    target: &BucketTarget,
    owner: Option<&str>,
    verify_owner: bool,
) -> Result<()> {
    validate_revision(session, target.revision)?;
    if target.connection != session.profile.id
        || target.source != InventorySource::CephAdmin
        || target.name.is_empty()
        || target.name.contains(['/', ':'])
        || target.tenant.contains(['/', ':'])
        || target.instance.as_ref().is_none_or(String::is_empty)
    {
        return Err(AppError::Invalid(
            "Select an exact RGW bucket instance from this gateway.".into(),
        ));
    }
    let current = bucket_binding(client, target).await?;
    if current.bucket != target.name
        || current.tenant != target.tenant
        || Some(current.id.as_str()) != target.instance.as_deref()
        || (verify_owner && current.owner.as_deref() != owner)
    {
        return Err(AppError::Invalid(
            "The bucket tenant, instance or owner binding changed. Refresh the inventory.".into(),
        ));
    }
    Ok(())
}

fn authorize_browser(session: &Session, target: &BucketTarget) -> Result<Outcome> {
    if target.tenant != session.context.tenant
        || !session.profile.settings.browser_enabled
        || matches!(
            session.profile.capability(IdentityKind::S3),
            CapabilityState::Unavailable | CapabilityState::Denied
        )
    {
        return Err(AppError::Invalid(
            "Browser requires an enabled S3 identity in the bucket tenant.".into(),
        ));
    }
    Ok(Outcome {
        success: true,
        status: Some(200),
        code: None,
        message: "Bucket verified for the connection’s S3 Browser.".into(),
        details: None,
    })
}

fn confirm(actual: &str, expected: String) -> Result<()> {
    if actual != expected {
        return Err(AppError::Invalid(
            "Type the exact confirmation shown for this action.".into(),
        ));
    }
    Ok(())
}

fn enable(fields: &mut Fields, name: &'static str, enabled: bool) {
    if enabled {
        fields.push((name, "true".into()));
    }
}

pub(crate) async fn check_index_batch(
    session: &Session,
    targets: Vec<BucketTarget>,
    mut receive: impl FnMut(IndexOutcome) -> Result<()>,
) -> Result<()> {
    if !(1..=200).contains(&targets.len()) {
        return Err(AppError::Invalid(
            "Select between 1 and 200 bucket instances.".into(),
        ));
    }
    let mut seen = HashSet::new();
    if targets
        .iter()
        .any(|target| !seen.insert((&target.tenant, &target.name)))
    {
        return Err(AppError::Invalid(
            "Select each tenant and bucket only once.".into(),
        ));
    }
    let client = RgwClient::new(session)?;
    for target in targets {
        let result = change_bucket(
            session,
            &client,
            &target,
            None,
            BucketChange::Index {
                repair: false,
                check_objects: false,
            },
            "",
        )
        .await
        .unwrap_or_else(rejected);
        receive(IndexOutcome { target, result })?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/administration/ceph_operations.rs"]
mod ceph_operations_tests;
