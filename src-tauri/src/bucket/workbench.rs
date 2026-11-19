use std::collections::{BTreeMap, BTreeSet};

use reqwest::Method;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    administration::repository::rgw_request,
    common::error::{AppError, Result},
    connection::{
        model::{IdentityKind, Provider, Workspace},
        service::Session,
    },
};

use super::{model::BucketQuota, quota::QuotaDocument, service::BucketService};

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InventorySource {
    StorageOps,
    CephAdmin,
}

impl InventorySource {
    pub fn workspace(self) -> Workspace {
        match self {
            Self::StorageOps => Workspace::StorageOps,
            Self::CephAdmin => Workspace::CephAdmin,
        }
    }

    pub fn identity(self) -> IdentityKind {
        match self {
            Self::StorageOps => IdentityKind::S3,
            Self::CephAdmin => IdentityKind::CephAdmin,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketTarget {
    pub connection: Uuid,
    pub revision: Option<Uuid>,
    pub source: InventorySource,
    pub tenant: String,
    pub name: String,
    pub instance: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryBucket {
    name: String,
    tenant: String,
    instance: Option<String>,
    owner: Option<String>,
    created_at: Option<String>,
    bytes: Option<u64>,
    objects: Option<u64>,
    quota: Option<BucketQuota>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawBucket {
    Stats(RawBucketStats),
    Name(String),
}

#[derive(Deserialize)]
struct RawBucketStats {
    bucket: String,
    #[serde(default)]
    tenant: String,
    id: Option<String>,
    owner: Option<String>,
    creation_time: Option<String>,
    usage: Option<BTreeMap<String, Category>>,
    bucket_quota: Option<QuotaDocument>,
}

#[derive(Deserialize)]
struct Category {
    size: Option<u64>,
    num_objects: Option<u64>,
}

impl RawBucket {
    fn project(self) -> InventoryBucket {
        match self {
            Self::Stats(stats) => {
                let category = stats.usage.as_ref().and_then(|usage| usage.get("rgw.main"));
                let empty = stats.usage.as_ref().is_some_and(BTreeMap::is_empty);
                InventoryBucket {
                    name: stats.bucket,
                    tenant: stats.tenant,
                    instance: stats.id.filter(|id| !id.is_empty()),
                    owner: stats.owner,
                    created_at: stats.creation_time,
                    bytes: category.and_then(|item| item.size).or(empty.then_some(0)),
                    objects: category
                        .and_then(|item| item.num_objects)
                        .or(empty.then_some(0)),
                    quota: stats
                        .bucket_quota
                        .and_then(|quota| match quota.editor_values() {
                            Ok(value) => Some(value),
                            Err(error) => {
                                tracing::debug!(%error, "Invalid RGW inventory quota");
                                None
                            }
                        }),
                }
            }
            Self::Name(qualified) => {
                let (tenant, name) = qualified.split_once('/').unwrap_or(("", &qualified));
                InventoryBucket {
                    name: name.into(),
                    tenant: tenant.into(),
                    instance: None,
                    owner: None,
                    created_at: None,
                    bytes: None,
                    objects: None,
                    quota: None,
                }
            }
        }
    }
}

pub async fn list(
    session: &Session,
    source: InventorySource,
    mut receive: impl FnMut(Vec<InventoryBucket>) -> Result<()>,
) -> Result<()> {
    if source == InventorySource::CephAdmin {
        return list_rgw(session, receive).await;
    }
    BucketService::list_pages(session, |page| {
        receive(
            page.into_iter()
                .map(|bucket| InventoryBucket {
                    name: bucket.name,
                    tenant: session.context.tenant.clone(),
                    instance: None,
                    owner: None,
                    created_at: bucket.created_at,
                    bytes: None,
                    objects: None,
                    quota: None,
                })
                .collect(),
        )
    })
    .await
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawInventory {
    Page {
        buckets: Vec<RawBucket>,
        truncated: bool,
        marker: Option<String>,
    },
    Complete(Vec<RawBucket>),
}

async fn list_rgw(
    session: &Session,
    mut receive: impl FnMut(Vec<InventoryBucket>) -> Result<()>,
) -> Result<()> {
    let mut marker = String::new();
    let mut visited = BTreeSet::new();
    loop {
        let mut query = vec![("stats", "true"), ("max-entries", "200")];
        if !marker.is_empty() {
            query.push(("marker", marker.as_str()));
        }
        let body = rgw_request(session, Method::GET, "bucket", query).await?;
        let page: RawInventory = serde_json::from_str(&body).map_err(|_| {
            AppError::Storage("The gateway returned an invalid bucket inventory.".into())
        })?;
        let (buckets, next) = match page {
            RawInventory::Complete(buckets) => (buckets, None),
            RawInventory::Page {
                buckets,
                truncated: false,
                ..
            } => (buckets, None),
            RawInventory::Page {
                buckets,
                truncated: true,
                marker,
            } => {
                let next = marker
                    .filter(|value| !value.is_empty() && visited.insert(value.clone()))
                    .ok_or_else(|| {
                        AppError::Storage(
                            "The gateway returned an invalid bucket inventory marker.".into(),
                        )
                    })?;
                (buckets, Some(next))
            }
        };
        receive(buckets.into_iter().map(RawBucket::project).collect())?;
        let Some(next) = next else {
            return Ok(());
        };
        marker = next;
    }
}

pub fn validate_revision(session: &Session, revision: Option<Uuid>) -> Result<()> {
    if session.profile.revision != revision {
        return Err(AppError::Invalid(
            "The connection changed. Refresh the bucket inventory.".into(),
        ));
    }
    Ok(())
}

pub fn validate_target(session: &Session, target: &BucketTarget) -> Result<()> {
    validate_revision(session, target.revision)?;
    if session.context.connection_id != target.connection
        || target.tenant != session.context.tenant
        || target.name.is_empty()
        || target.name.contains(['/', ':'])
        || (target.source == InventorySource::CephAdmin
            && session.profile.provider != Provider::Ceph)
        || (target.source == InventorySource::StorageOps
            && !session.profile.settings.manager_enabled)
    {
        return Err(AppError::Invalid(
            "The selected bucket is outside this connection’s configuration scope.".into(),
        ));
    }
    Ok(())
}

pub async fn validate_instance(session: &Session, target: &BucketTarget) -> Result<()> {
    validate_target(session, target)?;
    let expected = target
        .instance
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::Invalid("The gateway did not identify this bucket instance.".into())
        })?;
    let qualified = if target.tenant.is_empty() {
        target.name.clone()
    } else {
        format!("{}/{}", target.tenant, target.name)
    };
    let response = rgw_request(
        session,
        Method::GET,
        "bucket",
        [("bucket", qualified.as_str()), ("stats", "true")],
    )
    .await?;
    let current: RawBucketStats = serde_json::from_str(&response)
        .map_err(|_| AppError::Storage("The gateway returned invalid bucket statistics.".into()))?;
    if current.tenant != target.tenant
        || current.bucket != target.name
        || current.id.as_deref() != Some(expected)
    {
        return Err(AppError::Invalid(
            "The bucket instance changed. Refresh the inventory.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/bucket/workbench.rs"]
mod bucket_workbench;
