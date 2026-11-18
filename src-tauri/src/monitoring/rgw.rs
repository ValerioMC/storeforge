use std::collections::BTreeMap;

use chrono::{DateTime, TimeDelta, Utc};
use reqwest::Method;
use serde::Deserialize;

use crate::{administration::repository::rgw_request, connection::service::Session};

use super::{
    collection::reading_error,
    model::{Coverage, Quota, Reading, RgwStorage, Traffic, TrafficPeriod},
};

#[derive(Deserialize)]
struct UsageDocument {
    summary: Option<Vec<UsageOwner>>,
}

#[derive(Deserialize)]
struct UsageOwner {
    total: Option<UsageTotals>,
}

#[derive(Deserialize)]
struct UsageTotals {
    bytes_sent: Option<u64>,
    bytes_received: Option<u64>,
    ops: Option<u64>,
    successful_ops: Option<u64>,
}

#[derive(Deserialize)]
struct BucketDocument {
    bucket_quota: Option<QuotaDocument>,
    usage: Option<BTreeMap<String, StorageCategory>>,
}

#[derive(Deserialize)]
struct QuotaDocument {
    enabled: bool,
    max_size: Option<i64>,
    max_size_kb: Option<i64>,
    max_objects: Option<i64>,
}

#[derive(Deserialize)]
struct StorageCategory {
    size: u64,
    num_objects: u64,
}

pub async fn traffic(session: &Session, now: DateTime<Utc>) -> Reading<TrafficPeriod> {
    let end = DateTime::from_timestamp(now.timestamp().div_euclid(3600) * 3600, 0).unwrap_or(now);
    let start = end - TimeDelta::hours(24);
    let filters = [
        ("start", start.format("%Y-%m-%d %H:%M:%S").to_string()),
        ("end", end.format("%Y-%m-%d %H:%M:%S").to_string()),
        ("show-entries", "false".into()),
        ("show-summary", "true".into()),
    ];
    match rgw_request(session, Method::GET, "usage", filters).await {
        Ok(document) => traffic_totals(&document, start, end),
        Err(error) => reading_error(error),
    }
}

fn traffic_totals(
    document: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Reading<TrafficPeriod> {
    let Ok(UsageDocument {
        summary: Some(owners),
    }) = serde_json::from_str(document)
    else {
        return Reading::missing(
            Coverage::Missing,
            "The gateway did not report traffic totals.",
        );
    };
    if owners.is_empty() {
        return Reading::missing(
            Coverage::Missing,
            "No usage log entries were reported for this period.",
        );
    }
    let mut totals = Traffic {
        bytes_sent: Some(0),
        bytes_received: Some(0),
        requests: Some(0),
        successful_requests: Some(0),
    };
    for owner in owners {
        let Some(owner) = owner.total else {
            return Reading::missing(
                Coverage::Partial,
                "Some owners have no gateway usage totals.",
            );
        };
        totals.bytes_sent = sum(totals.bytes_sent, owner.bytes_sent);
        totals.bytes_received = sum(totals.bytes_received, owner.bytes_received);
        totals.requests = sum(totals.requests, owner.ops);
        totals.successful_requests = sum(totals.successful_requests, owner.successful_ops);
    }
    let complete = [
        totals.bytes_sent,
        totals.bytes_received,
        totals.requests,
        totals.successful_requests,
    ]
    .iter()
    .all(Option::is_some);
    let period = TrafficPeriod { start, end, totals };
    if complete {
        Reading::complete(period)
    } else {
        Reading::new(
            Coverage::Partial,
            Some(period),
            Some("The gateway omitted some traffic counters.".into()),
        )
    }
}

fn sum(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    left.zip(right)
        .map(|(left, right)| left.saturating_add(right))
}

pub async fn bucket_info(session: &Session, bucket: &str) -> (Reading<Quota>, Reading<RgwStorage>) {
    let tenant = &session.context.tenant;
    let identifier = if tenant.is_empty() || bucket.starts_with(&format!("{tenant}/")) {
        bucket.to_owned()
    } else {
        format!("{tenant}/{bucket}")
    };
    let filters = [("bucket", identifier.as_str()), ("stats", "true")];
    let response = rgw_request(session, Method::GET, "bucket", filters).await;
    match response {
        Ok(document) => bucket_readings(&document),
        Err(error) => {
            let coverage = super::collection::error_coverage(&error);
            let message = error.to_string();
            (
                Reading::missing(coverage, &message),
                Reading::missing(coverage, message),
            )
        }
    }
}

fn bucket_readings(document: &str) -> (Reading<Quota>, Reading<RgwStorage>) {
    let document = serde_json::from_str::<BucketDocument>(document).ok();
    let quota = document
        .as_ref()
        .and_then(|value| value.bucket_quota.as_ref())
        .map(|quota| Quota {
            enabled: quota.enabled,
            max_bytes: quota
                .max_size
                .and_then(|bytes| u64::try_from(bytes).ok())
                .or_else(|| {
                    quota
                        .max_size
                        .is_none()
                        .then_some(quota.max_size_kb)
                        .flatten()
                        .and_then(|value| u64::try_from(value).ok()?.checked_mul(1024))
                }),
            max_objects: quota
                .max_objects
                .and_then(|value| u64::try_from(value).ok()),
        });
    let storage = document.and_then(|value| value.usage).map(|categories| {
        categories.into_values().fold(
            RgwStorage {
                bytes: 0,
                objects: 0,
            },
            |storage, category| RgwStorage {
                bytes: storage.bytes.saturating_add(category.size),
                objects: storage.objects.saturating_add(category.num_objects),
            },
        )
    });
    (
        quota.map_or_else(
            || {
                Reading::missing(
                    Coverage::Missing,
                    "The gateway did not report a bucket quota.",
                )
            },
            Reading::complete,
        ),
        storage.map_or_else(
            || {
                Reading::missing(
                    Coverage::Missing,
                    "The gateway did not report bucket storage.",
                )
            },
            Reading::complete,
        ),
    )
}
