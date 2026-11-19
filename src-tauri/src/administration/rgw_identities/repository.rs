use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use reqwest::Method;
use serde::{Deserialize, Deserializer, de::DeserializeOwned};

use crate::{
    administration::repository::rgw_request,
    common::{
        document::{read_document, write_document},
        error::{AppError, Result},
    },
    connection::service::Session,
};

use super::model::*;

pub type Fields = Vec<(&'static str, String)>;

pub struct RegistryRepository(PathBuf);

impl RegistryRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self(directory.join("rgw-identities.json"))
    }

    pub fn read(&self) -> Result<Registry> {
        read_document(&self.0)
    }

    pub fn write(&self, registry: &Registry) -> Result<()> {
        write_document(self.0.parent().unwrap_or(Path::new(".")), &self.0, registry)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ListDocument<T> {
    Page {
        keys: Vec<T>,
        truncated: bool,
        marker: Option<String>,
    },
    Legacy(Vec<T>),
}

fn page<T>(document: ListDocument<T>, marker: Option<&str>) -> Result<(Vec<T>, Option<String>)> {
    match document {
        ListDocument::Page {
            keys,
            truncated: false,
            ..
        } => Ok((keys, None)),
        ListDocument::Page {
            keys,
            truncated: true,
            marker: next,
        } => {
            let next = next
                .filter(|next| !next.is_empty() && Some(next.as_str()) != marker)
                .ok_or_else(|| AppError::Storage("RGW did not return a new page marker.".into()))?;
            Ok((keys, Some(next)))
        }
        ListDocument::Legacy(keys) => {
            let start = match marker {
                Some(marker) => marker
                    .parse::<usize>()
                    .ok()
                    .filter(|offset| *offset <= keys.len())
                    .ok_or_else(|| {
                        AppError::Invalid(
                            "The inventory changed. Refresh it before continuing.".into(),
                        )
                    })?,
                None => 0,
            };
            let length = keys.len();
            let end = start.saturating_add(PAGE_SIZE);
            Ok((
                keys.into_iter().skip(start).take(PAGE_SIZE).collect(),
                (end < length).then(|| end.to_string()),
            ))
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum UserListing {
    Key(String),
    Entry(Box<UserDocument>),
}

impl UserListing {
    fn identity(self, default_tenant: &str) -> Result<UserIdentity> {
        match self {
            Self::Key(key) => parse_identity(&key, default_tenant),
            Self::Entry(entry) => entry.identity(Some(default_tenant)),
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct UserDocument {
    uid: Option<String>,
    user_id: Option<String>,
    user: Option<Box<UserField>>,
    tenant: Option<String>,
    display_name: String,
    email: String,
    account_id: Option<String>,
    account: Option<String>,
    #[serde(deserialize_with = "optional_bool", alias = "account-root")]
    account_root: Option<bool>,
    #[serde(rename = "type")]
    native_type: Option<String>,
    #[serde(deserialize_with = "optional_bool")]
    suspended: Option<bool>,
    max_buckets: Option<i32>,
    user_quota: Option<QuotaDocument>,
    bucket_quota: Option<QuotaDocument>,
    stats: Option<UsageDocument>,
    keys: Option<Vec<KeyDocument>>,
    s3_credentials: Option<Vec<KeyDocument>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum UserField {
    Name(String),
    Document(Box<UserDocument>),
}

impl UserDocument {
    fn detail(&self, expected: Option<&UserIdentity>) -> Result<UserDetail> {
        let inner = self.inner();
        let identity = self.identity(expected.map(|owner| owner.tenant.as_str()))?;
        if expected.is_some_and(|expected| *expected != identity) {
            return Err(identity_mismatch());
        }
        let account = asserted(
            [
                self.account_id.as_deref(),
                self.account.as_deref(),
                inner.account_id.as_deref(),
                inner.account.as_deref(),
            ]
            .into_iter()
            .flatten()
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        )?;
        let account_root = asserted(
            [
                self.account_root,
                inner.account_root,
                self.native_type.as_ref().map(|kind| kind == "root"),
                inner.native_type.as_ref().map(|kind| kind == "root"),
            ]
            .into_iter()
            .flatten(),
        )?;
        let keys = if [self, inner]
            .iter()
            .any(|user| user.keys.is_some() || user.s3_credentials.is_some())
        {
            let keys = key_views(
                [self, inner].into_iter().flat_map(UserDocument::keys),
                &identity,
            )?;
            Some(keys)
        } else {
            None
        };
        Ok(UserDetail {
            identity,
            account,
            account_root,
            display_name: inner.display_name.clone(),
            email: inner.email.clone(),
            suspended: inner.suspended,
            max_buckets: inner.max_buckets,
            quota: inner.user_quota.as_ref().map(QuotaDocument::quota),
            bucket_quota: inner.bucket_quota.as_ref().map(QuotaDocument::quota),
            usage: inner.stats.as_ref().map(UsageDocument::usage),
            keys,
            connection_key_id: None,
        })
    }

    fn inner(&self) -> &Self {
        match self.user.as_deref() {
            Some(UserField::Document(user)) => user,
            _ => self,
        }
    }

    fn keys(&self) -> impl Iterator<Item = &KeyDocument> {
        self.keys.iter().chain(&self.s3_credentials).flatten()
    }

    fn identity(&self, default_tenant: Option<&str>) -> Result<UserIdentity> {
        let inner = self.inner();
        let identifiers = [self, inner]
            .into_iter()
            .flat_map(|user| {
                [
                    user.uid.as_deref(),
                    user.user_id.as_deref(),
                    match user.user.as_deref() {
                        Some(UserField::Name(name)) => Some(name),
                        _ => None,
                    },
                ]
            })
            .flatten();
        let (uids, embedded): (Vec<_>, Vec<_>) = identifiers
            .map(|key| match key.split_once('$') {
                Some((tenant, uid)) => (uid, Some(tenant)),
                None => (key, None),
            })
            .unzip();
        let tenant = asserted(
            [self.tenant.as_deref(), inner.tenant.as_deref()]
                .into_iter()
                .chain(embedded)
                .flatten(),
        )?
        .or(default_tenant)
        .unwrap_or_default();
        let uid = asserted(uids.into_iter())?.ok_or_else(identity_mismatch)?;
        parse_identity(uid, tenant)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AccountListing {
    Key(String),
    Entry(Box<AccountDocument>),
}

impl AccountListing {
    fn id(self) -> Result<String> {
        match self {
            Self::Key(id) if !id.is_empty() => Ok(id),
            Self::Key(_) => Err(identity_mismatch()),
            Self::Entry(entry) => entry.id(),
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct AccountDocument {
    id: Option<String>,
    account_id: Option<String>,
    account: Option<Box<AccountDocument>>,
    tenant: Option<String>,
    #[serde(alias = "account_name")]
    name: String,
    email: String,
    max_users: Option<i32>,
    max_buckets: Option<i32>,
    max_roles: Option<i32>,
    max_groups: Option<i32>,
    max_access_keys: Option<i32>,
    #[serde(alias = "quota")]
    account_quota: Option<QuotaDocument>,
    bucket_quota: Option<QuotaDocument>,
    stats: Option<UsageDocument>,
    #[serde(alias = "users_count", alias = "num_users")]
    user_count: Option<u64>,
    #[serde(alias = "buckets_count", alias = "num_buckets")]
    bucket_count: Option<u64>,
    users: Option<AccountUsers>,
    user_list: Option<Vec<UserListing>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AccountUsers {
    Listing(Vec<UserListing>),
    Count(u64),
}

impl AccountDocument {
    fn id(&self) -> Result<String> {
        let nested = self.account.as_deref();
        asserted(
            [
                self.id.as_ref(),
                self.account_id.as_ref(),
                nested.and_then(|a| a.id.as_ref()),
                nested.and_then(|a| a.account_id.as_ref()),
            ]
            .into_iter()
            .flatten()
            .filter(|id| !id.is_empty())
            .cloned(),
        )?
        .ok_or_else(identity_mismatch)
    }

    fn detail(mut self, expected: Option<&str>) -> Result<AccountDetail> {
        let id = self.id()?;
        if expected.is_some_and(|expected| expected != id) {
            return Err(identity_mismatch());
        }
        let tenant = asserted(
            [
                self.tenant.as_ref(),
                self.account
                    .as_ref()
                    .and_then(|account| account.tenant.as_ref()),
            ]
            .into_iter()
            .flatten()
            .cloned(),
        )?
        .unwrap_or_default();
        let info = self.account.take().map_or(self, |account| *account);
        let (reported_users, listed_users) = match info.users {
            Some(AccountUsers::Count(count)) => (Some(count), None),
            Some(AccountUsers::Listing(users)) => (None, Some(users)),
            None => (None, None),
        };
        let users = info
            .user_list
            .or(listed_users)
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|entry| entry.identity(&tenant))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?;
        let user_count = info
            .user_count
            .or(reported_users)
            .or_else(|| info.stats.as_ref().and_then(|usage| usage.users))
            .or_else(|| users.as_ref().map(|users| users.len() as u64));
        let mut usage = info.stats.as_ref().map(UsageDocument::usage);
        if let Some(buckets) = info.bucket_count {
            usage
                .get_or_insert(Usage {
                    bytes: None,
                    objects: None,
                    buckets: None,
                })
                .buckets = Some(buckets);
        }
        Ok(AccountDetail {
            id,
            tenant,
            users,
            root: None,
            user_count,
            name: info.name,
            email: info.email,
            limits: AccountLimits {
                max_users: info.max_users,
                max_buckets: info.max_buckets,
                max_roles: info.max_roles,
                max_groups: info.max_groups,
                max_access_keys: info.max_access_keys,
            },
            quota: info.account_quota.as_ref().map(QuotaDocument::quota),
            bucket_quota: info.bucket_quota.as_ref().map(QuotaDocument::quota),
            usage,
        })
    }
}

#[derive(Deserialize)]
struct QuotaDocument {
    #[serde(deserialize_with = "optional_bool", default)]
    enabled: Option<bool>,
    #[serde(alias = "max_size_bytes")]
    max_size: Option<i64>,
    max_size_kb: Option<i64>,
    max_objects: Option<i64>,
}

impl QuotaDocument {
    fn quota(&self) -> Quota {
        Quota {
            enabled: self.enabled.unwrap_or(false),
            max_bytes: self.max_size.or_else(|| {
                self.max_size_kb.and_then(|size| {
                    if size < 0 {
                        Some(-1)
                    } else {
                        size.checked_mul(1024)
                    }
                })
            }),
            max_objects: self.max_objects,
        }
    }
}

#[derive(Deserialize)]
struct UsageDocument {
    #[serde(alias = "size", alias = "total_bytes")]
    size_bytes: Option<u64>,
    #[serde(alias = "num_objects", alias = "total_entries")]
    objects: Option<u64>,
    #[serde(alias = "num_buckets", alias = "bucket_count", alias = "buckets_count")]
    buckets: Option<u64>,
    #[serde(alias = "num_users", alias = "user_count", alias = "users_count")]
    users: Option<u64>,
}

impl UsageDocument {
    fn usage(&self) -> Usage {
        Usage {
            bytes: self.size_bytes,
            objects: self.objects,
            buckets: self.buckets,
        }
    }
}

#[derive(Deserialize)]
struct KeyDocument {
    access_key: String,
    secret_key: Option<String>,
    user: Option<String>,
    #[serde(default, deserialize_with = "optional_bool")]
    active: Option<bool>,
    #[serde(alias = "key_status")]
    status: Option<String>,
}

impl KeyDocument {
    fn state(&self) -> Result<Option<bool>> {
        let status = match self.status.as_deref() {
            Some(status) if status.eq_ignore_ascii_case("active") => Some(true),
            Some(status) if status.eq_ignore_ascii_case("inactive") => Some(false),
            _ => None,
        };
        asserted([self.active, status].into_iter().flatten())
    }

    fn verify_owner(&self, owner: &UserIdentity) -> Result<()> {
        if let Some(user) = &self.user {
            let observed = parse_identity(
                user.split_once(':').map_or(user.as_str(), |(uid, _)| uid),
                &owner.tenant,
            )?;
            if &observed != owner {
                return Err(identity_mismatch());
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CreatedKeyDocument {
    List(Vec<KeyDocument>),
    Single(KeyDocument),
    Entries(Box<UserDocument>),
}

pub fn issued_key(
    document: &str,
    owner: &UserIdentity,
    previous: &HashSet<String>,
) -> Result<IssuedKey> {
    let response: CreatedKeyDocument = decode(document)?;
    let entries: Vec<&KeyDocument> = match &response {
        CreatedKeyDocument::List(keys) => keys.iter().collect(),
        CreatedKeyDocument::Entries(user) => user.keys().chain(user.inner().keys()).collect(),
        CreatedKeyDocument::Single(key) => vec![key],
    };
    let mut new = HashMap::new();
    for key in entries
        .into_iter()
        .filter(|key| !previous.contains(&key.access_key))
    {
        key.verify_owner(owner)?;
        if let Some(secret) = key
            .secret_key
            .as_deref()
            .filter(|secret| !secret.is_empty())
        {
            new.insert(key.access_key.as_str(), secret);
        }
    }
    if new.len() != 1 {
        return Err(AppError::AmbiguousWrite("RGW did not identify exactly one newly issued key. Refresh the owner's key list before retrying.".into()));
    }
    let (access_key, secret_key) = new.into_iter().next().ok_or_else(identity_mismatch)?;
    Ok(IssuedKey {
        access_key: access_key.into(),
        secret_key: secret_key.into(),
    })
}

fn key_views<'a>(
    keys: impl Iterator<Item = &'a KeyDocument>,
    owner: &UserIdentity,
) -> Result<Vec<AccessKey>> {
    let mut views: Vec<AccessKey> = Vec::new();
    for key in keys {
        key.verify_owner(owner)?;
        let active = key.state()?;
        if let Some(view) = views.iter_mut().find(|view| view.id == key.access_key) {
            view.active = asserted([view.active, active].into_iter().flatten())?;
        } else if !key.access_key.is_empty() {
            views.push(AccessKey {
                id: key.access_key.clone(),
                active,
                metadata: KeyMetadata::default(),
                enforcement: None,
                private_profile: None,
                issue: None,
                provider_missing: false,
            });
        }
    }
    Ok(views)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BoolValue {
    Boolean(bool),
    Integer(u8),
    Text(String),
}

fn optional_bool<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<bool>, D::Error> {
    match Option::<BoolValue>::deserialize(deserializer)? {
        None => Ok(None),
        Some(BoolValue::Boolean(value)) => Ok(Some(value)),
        Some(BoolValue::Integer(0)) => Ok(Some(false)),
        Some(BoolValue::Integer(1)) => Ok(Some(true)),
        Some(BoolValue::Text(value)) if value.eq_ignore_ascii_case("true") || value == "1" => {
            Ok(Some(true))
        }
        Some(BoolValue::Text(value)) if value.eq_ignore_ascii_case("false") || value == "0" => {
            Ok(Some(false))
        }
        _ => Err(serde::de::Error::custom("unexpected RGW flag")),
    }
}

fn asserted<T: PartialEq>(mut values: impl Iterator<Item = T>) -> Result<Option<T>> {
    let first = values.next();
    if values.any(|value| Some(&value) != first.as_ref()) {
        return Err(identity_mismatch());
    }
    Ok(first)
}

fn identity_mismatch() -> AppError {
    AppError::Storage(
        "RGW returned a missing or conflicting resource identity or key state.".into(),
    )
}

pub fn parse_identity(text: &str, default_tenant: &str) -> Result<UserIdentity> {
    let (tenant, uid) = text.split_once('$').unwrap_or((default_tenant, text));
    if uid.is_empty()
        || uid.contains('$')
        || uid.contains(':')
        || tenant.contains('$')
        || text.starts_with('$')
        || uid.trim() != uid
        || tenant.trim() != tenant
    {
        return Err(AppError::Invalid(
            "Use an exact RGW UID in tenant$uid form for a tenant user.".into(),
        ));
    }
    Ok(UserIdentity {
        tenant: tenant.into(),
        uid: uid.into(),
    })
}

const PAGE_SIZE: usize = 50;

pub async fn list(
    session: &Session,
    kind: IdentityKind,
    marker: Option<&str>,
) -> Result<(Vec<IdentityTarget>, Option<String>)> {
    let mut fields = vec![("max-entries", PAGE_SIZE.to_string())];
    add(&mut fields, "marker", marker);
    match kind {
        IdentityKind::Account => {
            let document = call(session, Method::GET, "metadata/account", fields).await?;
            let (entries, next) = page::<AccountListing>(decode(&document)?, marker)?;
            let rows = entries
                .into_iter()
                .map(|entry| Ok(IdentityTarget::Account { id: entry.id()? }))
                .collect::<Result<_>>()?;
            Ok((rows, next))
        }
        IdentityKind::User => {
            fields.push(("list", String::new()));
            let document = call(session, Method::GET, "user", fields).await?;
            let (entries, next) = page::<UserListing>(decode(&document)?, marker)?;
            let rows = entries
                .into_iter()
                .map(|entry| {
                    Ok(IdentityTarget::User {
                        identity: entry.identity("")?,
                    })
                })
                .collect::<Result<_>>()?;
            Ok((rows, next))
        }
    }
}

pub fn registered_page(entries: Vec<InventoryRow>, marker: Option<&str>) -> Result<InventoryPage> {
    let (items, marker) = page(ListDocument::Legacy(entries), marker)?;
    Ok(InventoryPage { items, marker })
}

pub async fn account(session: &Session, id: &str) -> Result<AccountDetail> {
    let document = call(session, Method::GET, "account", vec![("id", id.into())]).await?;
    decode::<AccountDocument>(&document)?.detail(Some(id))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BucketStatsDocument {
    List(Vec<BucketStats>),
    Page {
        buckets: Vec<BucketStats>,
        #[serde(default)]
        truncated: bool,
        marker: Option<String>,
    },
}

#[derive(Deserialize)]
struct BucketStats {
    owner: Option<String>,
    #[serde(alias = "account-id")]
    account_id: Option<String>,
    tenant: Option<String>,
    usage: Option<BTreeMap<String, BucketCategory>>,
}

#[derive(Deserialize)]
struct BucketCategory {
    size: Option<u64>,
    size_actual: Option<u64>,
    num_objects: Option<u64>,
}

pub async fn account_usage(session: &Session, account: &AccountDetail) -> Result<Usage> {
    let document = call(
        session,
        Method::GET,
        "bucket",
        vec![("stats", "true".into())],
    )
    .await?;
    let buckets =
        match decode::<BucketStatsDocument>(&document)? {
            BucketStatsDocument::List(buckets) => buckets,
            BucketStatsDocument::Page {
                buckets,
                truncated: false,
                marker,
            } if marker.as_deref().is_none_or(str::is_empty) => buckets,
            BucketStatsDocument::Page { .. } => return Err(AppError::Storage(
                "The gateway returned a partial bucket inventory. Account usage is unavailable."
                    .into(),
            )),
        };
    let mut usage = Usage {
        bytes: Some(0),
        objects: Some(0),
        buckets: Some(0),
    };
    for bucket in buckets {
        let owner = bucket
            .account_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .or(bucket.owner.as_deref().filter(|owner| !owner.is_empty()))
            .ok_or_else(|| {
                AppError::Storage(
                    "RGW omitted bucket ownership. Account usage could not be verified.".into(),
                )
            })?;
        if owner != account.id
            || bucket
                .tenant
                .as_ref()
                .is_some_and(|tenant| tenant != &account.tenant)
        {
            continue;
        }
        usage.buckets = sum(usage.buckets, Some(1));
        if let Some(categories) = bucket.usage {
            for category in categories.into_values() {
                usage.bytes = sum(usage.bytes, category.size.or(category.size_actual));
                usage.objects = sum(usage.objects, category.num_objects);
            }
        } else {
            usage.bytes = None;
            usage.objects = None;
        }
    }
    Ok(usage)
}

fn sum(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    left.zip(right)
        .and_then(|(left, right)| left.checked_add(right))
}

pub async fn user(session: &Session, identity: &UserIdentity, stats: bool) -> Result<UserDetail> {
    let mut fields = user_fields(identity)?;
    if stats {
        fields.push(("stats", "true".into()));
    }
    let document = call(session, Method::GET, "user", fields).await?;
    decode::<UserDocument>(&document)?.detail(Some(identity))
}

pub async fn owner(session: &Session, access_key: &str) -> Result<UserDetail> {
    let document = call(
        session,
        Method::GET,
        "user",
        vec![("access-key", access_key.into())],
    )
    .await?;
    let mut user = decode::<UserDocument>(&document)?.detail(None)?;
    if user
        .keys
        .as_ref()
        .is_some_and(|keys| keys.iter().any(|key| key.id == access_key))
    {
        user.connection_key_id = Some(access_key.into());
        Ok(user)
    } else {
        Err(AppError::Invalid(
            "The S3 credential does not identify a verifiable RGW user on this endpoint.".into(),
        ))
    }
}

pub fn user_fields(identity: &UserIdentity) -> Result<Fields> {
    if parse_identity(&identity.qualified(), "")? != *identity {
        return Err(identity_mismatch());
    }
    let mut fields = vec![("uid", identity.qualified())];
    if !identity.tenant.is_empty() {
        fields.push(("tenant", identity.tenant.clone()));
    }
    Ok(fields)
}

pub fn add<T: ToString>(fields: &mut Fields, name: &'static str, value: Option<T>) {
    if let Some(value) = value {
        fields.push((name, value.to_string()));
    }
}

pub fn decode<T: DeserializeOwned>(document: &str) -> Result<T> {
    serde_json::from_str(document)
        .map_err(|_| AppError::Storage("RGW returned an unexpected identity response.".into()))
}

pub fn created_account(document: &str) -> Result<AccountDetail> {
    decode::<AccountDocument>(document)?.detail(None)
}

pub async fn call(
    session: &Session,
    method: Method,
    resource: &str,
    fields: Fields,
) -> Result<String> {
    rgw_request(session, method, resource, fields)
        .await
        .map_err(sanitized_error)
}

fn sanitized_error(error: AppError) -> AppError {
    let sanitize = |message: String| {
        let status = message
            .split_once(' ')
            .and_then(|(value, _)| value.parse::<u16>().ok());
        status.map_or_else(|| "The RGW identity request could not be completed.".into(),
            |status| format!("RGW declined the identity request (HTTP {status}). Check the endpoint's Admin Ops permissions and feature support."))
    };
    match error {
        AppError::Storage(message) => AppError::Storage(sanitize(message)),
        AppError::Missing(message) if resource_is_missing(&message) => {
            AppError::Missing(sanitize(message))
        }
        AppError::Missing(message) => AppError::Storage(sanitize(message)),
        error => error,
    }
}

#[derive(Deserialize)]
struct ErrorDocument {
    #[serde(rename = "Code", alias = "code")]
    code: String,
}

fn resource_is_missing(message: &str) -> bool {
    let Some((_, body)) = message.split_once('{') else {
        return false;
    };
    serde_json::from_str::<ErrorDocument>(&format!("{{{body}")).is_ok_and(|error| {
        matches!(
            error.code.as_str(),
            "NoSuchUser" | "NoSuchAccount" | "NoSuchKey" | "NoSuchAccessKey"
        )
    })
}
