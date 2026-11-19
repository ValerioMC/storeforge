use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    administration::repository::{rgw_request, rgw_request_with_body},
    common::error::{AppError, Result},
    connection::{
        model::{IdentityKind, Provider},
        service::Session,
    },
};

use super::model::{BucketQuota, BucketQuotaUpdate};

#[derive(Deserialize)]
struct BucketStats {
    bucket_quota: QuotaDocument,
}

#[derive(Deserialize, Serialize)]
pub(super) struct QuotaDocument {
    enabled: bool,
    #[serde(default)]
    check_on_raw: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_size: Option<i64>,
    #[serde(default, skip_serializing)]
    max_size_kb: Option<i64>,
    max_objects: i64,
}

impl QuotaDocument {
    pub(super) fn editor_values(&self) -> Result<BucketQuota> {
        let bytes = self
            .max_size
            .or_else(|| {
                self.max_size_kb.and_then(|limit| {
                    if limit < 0 {
                        Some(-1)
                    } else {
                        limit.checked_mul(1024)
                    }
                })
            })
            .ok_or_else(|| {
                AppError::Storage("The gateway returned an invalid bucket storage limit.".into())
            })?;
        Ok(BucketQuota {
            enabled: self.enabled,
            max_bytes: display_limit(bytes),
            max_objects: display_limit(self.max_objects),
        })
    }
}

pub async fn read(session: &Session, bucket: &str) -> Result<BucketQuota> {
    let target = quota_target(session, bucket)?;
    load(session, &target).await?.editor_values()
}

pub async fn write(session: &Session, bucket: &str, update: BucketQuotaUpdate) -> Result<()> {
    let max_size = parse_limit(&update.draft.max_bytes)?;
    let max_objects = parse_limit(&update.draft.max_objects)?;
    let target = quota_target(session, bucket)?;
    let current = load(session, &target).await?;
    if current.editor_values()? != update.expected {
        return Err(AppError::Invalid(
            "The gateway quota changed. Review the current limits before trying again.".into(),
        ));
    }
    let desired = QuotaDocument {
        enabled: update.draft.enabled,
        max_size: Some(max_size),
        max_objects,
        ..current
    };
    let body = serde_json::to_vec(&desired)?;
    rgw_request_with_body(
        session,
        Method::PUT,
        "bucket",
        [("bucket", target.as_str()), ("uid", ""), ("quota", "")],
        Some(body),
    )
    .await?;
    Ok(())
}

fn quota_target(session: &Session, bucket: &str) -> Result<String> {
    if session.profile.provider != Provider::Ceph
        || session.context.identity != IdentityKind::CephAdmin
    {
        return Err(AppError::Invalid(
            "Bucket quotas require a Ceph Admin identity.".into(),
        ));
    }
    if bucket.is_empty() || bucket.contains('/') {
        return Err(AppError::Invalid(
            "Choose a bucket from the current connection.".into(),
        ));
    }
    let tenant = &session.context.tenant;
    if tenant.is_empty() {
        return Ok(bucket.into());
    }
    Ok(format!("{tenant}/{bucket}"))
}

async fn load(session: &Session, target: &str) -> Result<QuotaDocument> {
    let response = rgw_request(
        session,
        Method::GET,
        "bucket",
        [("bucket", target), ("stats", "true")],
    )
    .await?;
    serde_json::from_str::<BucketStats>(&response)
        .map(|bucket| bucket.bucket_quota)
        .map_err(|_| {
            AppError::Storage("The gateway did not return a readable bucket quota.".into())
        })
}

fn display_limit(limit: i64) -> String {
    if limit < 0 {
        String::new()
    } else {
        limit.to_string()
    }
}

fn parse_limit(limit: &str) -> Result<i64> {
    if limit.is_empty() {
        return Ok(-1);
    }
    limit
        .parse::<i64>()
        .ok()
        .filter(|value| *value >= 0)
        .ok_or_else(|| {
            AppError::Invalid(
                "Enter a whole quota limit within the provider’s integer range.".into(),
            )
        })
}
