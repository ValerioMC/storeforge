use std::collections::{BTreeMap, HashSet};

use aws_sdk_s3::Client;

use crate::{
    common::error::{AppError, Result, storage_error},
    object::{
        model::{Location, ObjectId, ObjectQuery, ObjectRow, ObjectVersion},
        repository,
    },
};

use super::model::{Comparison, ComparisonRow, Difference, Integrity, JobFailure, Usage};

pub async fn objects(client: &Client, location: &Location) -> Result<Vec<ObjectRow>> {
    let mut rows = Vec::new();
    visit_objects(client, location, |page| rows.extend(page)).await?;
    Ok(rows)
}

pub async fn visit_objects(
    client: &Client,
    location: &Location,
    mut visit: impl FnMut(Vec<ObjectRow>),
) -> Result<()> {
    let mut query = ObjectQuery {
        location: location.clone(),
        cursor: None,
        recursive: true,
    };
    let mut seen = HashSet::new();
    loop {
        let page = repository::list(client, &query).await?;
        if let Some(cursor) = &page.cursor
            && !seen.insert(cursor.clone())
        {
            return Err(repeated_cursor());
        }
        visit(page.rows);
        if page.cursor.is_none() {
            return Ok(());
        }
        query.cursor = page.cursor;
    }
}

pub async fn visit_versions(
    client: &Client,
    location: &Location,
    mut visit: impl FnMut(Vec<ObjectVersion>),
) -> Result<()> {
    let mut seen = HashSet::new();
    let mut cursor = None;
    loop {
        let page =
            repository::versions(client, &location.bucket, &location.prefix, false, cursor).await?;
        if let Some(next) = &page.cursor
            && !seen.insert(next.clone())
        {
            return Err(repeated_cursor());
        }
        visit(page.versions);
        cursor = page.cursor;
        if cursor.is_none() {
            return Ok(());
        }
    }
}

fn repeated_cursor() -> AppError {
    AppError::Storage("The endpoint repeated a listing cursor.".into())
}

pub async fn all_versions(client: &Client, location: &Location) -> Result<Vec<ObjectId>> {
    let mut versions = Vec::new();
    visit_versions(client, location, |page| {
        versions.extend(page.into_iter().map(|version| ObjectId {
            bucket: location.bucket.clone(),
            key: version.key,
            version_id: Some(version.version_id),
        }));
    })
    .await?;
    Ok(versions)
}

pub async fn usage(client: &Client, location: &Location, include_versions: bool) -> Result<Usage> {
    let rows = objects(client, location).await?;
    let mut usage = Usage {
        objects: rows
            .iter()
            .filter(|row| !row.key.ends_with('/') || row.size != Some(0))
            .count(),
        bytes: rows
            .iter()
            .filter_map(|row| row.size.and_then(|size| u64::try_from(size).ok()))
            .sum(),
        versions: 0,
        delete_markers: 0,
    };
    if include_versions {
        count_versions(client, location, &mut usage).await?;
    }
    Ok(usage)
}

async fn count_versions(client: &Client, location: &Location, usage: &mut Usage) -> Result<()> {
    visit_versions(client, location, |page| {
        let markers = page.iter().filter(|version| version.delete_marker).count();
        usage.delete_markers += markers;
        usage.versions += page.len() - markers;
    })
    .await
}

pub async fn compare(
    source: &Client,
    source_location: &Location,
    target: &Client,
    target_location: &Location,
) -> Result<Comparison> {
    let (source, target) = tokio::try_join!(
        objects(source, source_location),
        objects(target, target_location)
    )?;
    Ok(compare_rows(
        &source,
        &source_location.prefix,
        &target,
        &target_location.prefix,
    ))
}

fn compare_rows(
    source: &[ObjectRow],
    source_prefix: &str,
    target: &[ObjectRow],
    target_prefix: &str,
) -> Comparison {
    let source = relative_rows(source, source_prefix);
    let mut target = relative_rows(target, target_prefix);
    let mut comparison = Comparison {
        matching: 0,
        changed: 0,
        different_etag: 0,
        source_only: 0,
        target_only: 0,
        differences: Vec::new(),
    };
    for (key, row) in source {
        match target.remove(key) {
            None => record_difference(&mut comparison, key, Difference::SourceOnly, row.size, None),
            Some(other) if row.size != other.size => record_difference(
                &mut comparison,
                key,
                Difference::Changed,
                row.size,
                other.size,
            ),
            Some(other) if row.etag != other.etag || row.etag.is_none() => record_difference(
                &mut comparison,
                key,
                Difference::DifferentEtag,
                row.size,
                other.size,
            ),
            Some(_) => comparison.matching += 1,
        }
    }
    for (key, row) in target {
        record_difference(&mut comparison, key, Difference::TargetOnly, None, row.size);
    }
    comparison
}

fn relative_rows<'a>(rows: &'a [ObjectRow], prefix: &str) -> BTreeMap<&'a str, &'a ObjectRow> {
    rows.iter()
        .filter_map(|row| Some((row.key.strip_prefix(prefix)?, row)))
        .collect()
}

fn record_difference(
    comparison: &mut Comparison,
    key: &str,
    difference: Difference,
    source_bytes: Option<i64>,
    target_bytes: Option<i64>,
) {
    match difference {
        Difference::SourceOnly => comparison.source_only += 1,
        Difference::TargetOnly => comparison.target_only += 1,
        Difference::Changed => comparison.changed += 1,
        Difference::DifferentEtag => comparison.different_etag += 1,
    }
    if comparison.differences.len() < 2000 {
        comparison.differences.push(ComparisonRow {
            key: key.into(),
            difference,
            source_bytes,
            target_bytes,
        });
    }
}

pub async fn integrity(client: &Client, location: &Location) -> Result<Integrity> {
    let rows = objects(client, location).await?;
    let mut result = Integrity {
        checked: rows.len(),
        failed: 0,
        failures: Vec::new(),
    };
    for row in rows {
        let check = client
            .head_object()
            .bucket(&location.bucket)
            .key(&row.key)
            .set_if_match(row.etag.clone())
            .send()
            .await
            .map_err(storage_error);
        let failure = match check {
            Ok(details) if details.content_length() != row.size => {
                Some("Listed and reported sizes differ.".into())
            }
            Ok(_) => None,
            Err(error) => Some(error.to_string()),
        };
        if let Some(message) = failure {
            result.failed += 1;
            if result.failures.len() < 2000 {
                result.failures.push(JobFailure {
                    item: row.key,
                    message,
                });
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "../../tests/unit/transfer/inventory.rs"]
mod tests;
