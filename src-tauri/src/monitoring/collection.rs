use std::{collections::BTreeMap, path::Path};

use aws_sdk_s3::Client;
use chrono::{DateTime, Utc};

use crate::{common::error::AppError, object::model::Location, transfer::inventory};

use super::model::{BucketUsage, Coverage, DistributionValue, Reading};

pub async fn bucket_usage(
    client: &Client,
    location: &Location,
    now: DateTime<Utc>,
) -> Reading<BucketUsage> {
    let mut versions = UsageAccumulator::new(true);
    let mut visited = false;
    let result = inventory::visit_versions(client, location, |page| {
        visited = true;
        for row in page {
            if row.delete_marker {
                versions.usage.delete_markers =
                    versions.usage.delete_markers.map(|count| count + 1);
            } else {
                versions.add(
                    &row.key,
                    row.size,
                    row.modified.as_deref(),
                    row.storage_class.as_deref(),
                    row.latest,
                    now,
                );
            }
        }
    })
    .await;
    match result {
        Ok(()) => versions.reading(Coverage::Complete, None),
        Err(error) if visited => versions.reading(Coverage::Partial, Some(error.to_string())),
        Err(error) => current_usage(client, location, now, error).await,
    }
}

async fn current_usage(
    client: &Client,
    location: &Location,
    now: DateTime<Utc>,
    version_error: AppError,
) -> Reading<BucketUsage> {
    let mut current = UsageAccumulator::new(false);
    let mut visited = false;
    let result = inventory::visit_objects(client, location, |page| {
        visited = true;
        for row in page.into_iter().filter(|row| !row.folder) {
            current.add(
                &row.key,
                row.size,
                row.modified.as_deref(),
                row.storage_class.as_deref(),
                true,
                now,
            );
        }
    })
    .await;
    match result {
        Ok(()) => current.reading(
            Coverage::CurrentOnly,
            Some(format!("Version listing: {version_error}")),
        ),
        Err(error) if visited => current.reading(Coverage::Partial, Some(error.to_string())),
        Err(error) => reading_error(error),
    }
}

struct UsageAccumulator {
    usage: BucketUsage,
    missing_size: bool,
}

impl UsageAccumulator {
    fn new(versions: bool) -> Self {
        Self {
            usage: BucketUsage {
                current_bytes: Some(0),
                current_objects: Some(0),
                noncurrent_bytes: versions.then_some(0),
                noncurrent_objects: versions.then_some(0),
                delete_markers: versions.then_some(0),
                ..Default::default()
            },
            missing_size: false,
        }
    }

    fn add(
        &mut self,
        key: &str,
        size: Option<i64>,
        modified: Option<&str>,
        storage_class: Option<&str>,
        current: bool,
        now: DateTime<Utc>,
    ) {
        let size = size.and_then(|bytes| u64::try_from(bytes).ok());
        self.missing_size |= size.is_none();
        let (bytes, objects) = if current {
            (
                &mut self.usage.current_bytes,
                &mut self.usage.current_objects,
            )
        } else {
            (
                &mut self.usage.noncurrent_bytes,
                &mut self.usage.noncurrent_objects,
            )
        };
        *bytes = bytes
            .zip(size)
            .map(|(total, value)| total.saturating_add(value));
        *objects = objects.map(|count| count.saturating_add(1));
        add_bin(&mut self.usage.sizes, size_band(size), size);
        add_bin(&mut self.usage.ages, age_band(modified, now), size);
        add_bin(&mut self.usage.types, content_type(key), size);
        add_bin(
            &mut self.usage.storage_classes,
            storage_class.unwrap_or("Unreported"),
            size,
        );
    }

    fn reading(self, coverage: Coverage, detail: Option<String>) -> Reading<BucketUsage> {
        let coverage = if self.missing_size {
            Coverage::Partial
        } else {
            coverage
        };
        let detail = detail.or_else(|| {
            self.missing_size
                .then(|| "The endpoint omitted some object sizes.".into())
        });
        Reading::new(coverage, Some(self.usage), detail)
    }
}

fn add_bin(distribution: &mut BTreeMap<String, DistributionValue>, label: &str, size: Option<u64>) {
    let bin = distribution.entry(label.into()).or_default();
    bin.objects = bin.objects.saturating_add(1);
    bin.bytes = bin.bytes.saturating_add(size.unwrap_or_default());
}

fn size_band(size: Option<u64>) -> &'static str {
    match size {
        Some(0..=1_048_575) => "Under 1 MiB",
        Some(1_048_576..=104_857_599) => "1–100 MiB",
        Some(104_857_600..=1_073_741_823) => "100 MiB–1 GiB",
        Some(_) => "1 GiB or more",
        None => "Unknown size",
    }
}

fn age_band(modified: Option<&str>, now: DateTime<Utc>) -> &'static str {
    match modified
        .and_then(|value| value.parse::<DateTime<Utc>>().ok())
        .map(|value| (now - value).num_days())
    {
        Some(i64::MIN..=6) => "Under 7 days",
        Some(7..=29) => "7–29 days",
        Some(30..=89) => "30–89 days",
        Some(_) => "90 days or more",
        None => "Unknown age",
    }
}

fn content_type(key: &str) -> &'static str {
    mime_guess::from_path(Path::new(key))
        .first_raw()
        .unwrap_or("application/octet-stream")
}

pub fn reading_error<T>(error: AppError) -> Reading<T> {
    Reading::missing(error_coverage(&error), error.to_string())
}

pub fn error_coverage(error: &AppError) -> Coverage {
    let message = error.to_string();
    if matches!(error, AppError::Invalid(_)) && message.contains("was denied by the endpoint") {
        return Coverage::Denied;
    }
    match message.split([' ', ':']).next().unwrap_or_default() {
        "401"
        | "403"
        | "AccessDenied"
        | "Forbidden"
        | "Unauthorized"
        | "InvalidAccessKeyId"
        | "SignatureDoesNotMatch"
        | "ExpiredToken"
        | "InvalidToken" => Coverage::Denied,
        _ => Coverage::Unavailable,
    }
}
