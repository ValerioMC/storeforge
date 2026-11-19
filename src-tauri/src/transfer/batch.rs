use std::collections::{BTreeMap, HashSet};

use aws_sdk_s3::Client;
use chrono::{DateTime, Utc};

use crate::{
    common::error::{AppError, Result},
    object::{
        model::{Location, ObjectChange, ObjectId, ObjectVersion},
        service::ObjectService,
    },
};

use super::{
    inventory,
    model::{VersionAction, VersionPlanItem, VersionRule},
};

pub async fn expand_selection(
    client: &Client,
    location: &Location,
    mut objects: Vec<ObjectId>,
    mut folders: Vec<String>,
) -> Result<Vec<ObjectId>> {
    if objects
        .iter()
        .any(|object| object.bucket != location.bucket || !object.key.starts_with(&location.prefix))
    {
        return Err(AppError::Invalid(
            "A selected object is outside the source location.".into(),
        ));
    }
    folders.sort();
    let mut covered = Vec::<String>::new();
    for folder in folders {
        if !folder.starts_with(&location.prefix) || !folder.ends_with('/') {
            return Err(AppError::Invalid(
                "A selected folder is outside the source location.".into(),
            ));
        }
        if covered
            .last()
            .is_some_and(|prefix| folder.starts_with(prefix))
        {
            continue;
        }
        let prefix = Location {
            prefix: folder.clone(),
            ..location.clone()
        };
        let rows = inventory::objects(client, &prefix).await?;
        objects.extend(
            rows.into_iter()
                .filter(|row| !row.folder && row.key.starts_with(&folder))
                .map(|row| ObjectId {
                    bucket: location.bucket.clone(),
                    key: row.key,
                    version_id: None,
                }),
        );
        covered.push(folder);
    }
    let mut seen = HashSet::new();
    objects.retain(|item| seen.insert(item.clone()));
    Ok(objects)
}

pub async fn prefix_history(client: &Client, location: &Location) -> Result<Vec<ObjectVersion>> {
    let mut rows = Vec::new();
    inventory::visit_versions(client, location, |page| {
        rows.extend(
            page.into_iter()
                .filter(|row| row.key.starts_with(&location.prefix)),
        );
    })
    .await?;
    Ok(rows)
}

fn modified(row: &ObjectVersion) -> Option<DateTime<Utc>> {
    row.modified.as_deref()?.parse().ok()
}

pub fn version_plan(rows: &[ObjectVersion], rule: &VersionRule) -> Result<Vec<VersionPlanItem>> {
    validate_rule(rule)?;
    let mut histories = BTreeMap::<&str, Vec<&ObjectVersion>>::new();
    for row in rows {
        histories.entry(&row.key).or_default().push(row);
    }
    let mut plan = Vec::new();
    for versions in histories.values_mut() {
        versions.sort_by_key(|row| {
            (
                std::cmp::Reverse(row.latest),
                std::cmp::Reverse(modified(row)),
            )
        });
        let Some(latest) = versions.first().filter(|row| row.latest) else {
            continue;
        };
        if versions.iter().skip(1).any(|row| row.latest) {
            return Err(AppError::Storage(
                "The provider reported multiple current versions.".into(),
            ));
        }
        match rule {
            VersionRule::RestoreDeleted if latest.delete_marker => {
                if let Some(row) = versions.iter().find(|row| !row.delete_marker) {
                    plan.push(plan_item(row, latest, VersionAction::Restore));
                }
            }
            VersionRule::RestoreAt { at, delete_missing } => {
                let previous = versions
                    .iter()
                    .find(|row| modified(row).is_some_and(|date| date <= *at));
                match previous {
                    Some(row) if !row.delete_marker && !row.latest => {
                        plan.push(plan_item(row, latest, VersionAction::Restore))
                    }
                    Some(row) if !row.delete_marker => (),
                    _ if *delete_missing && !latest.delete_marker => {
                        plan.push(plan_item(latest, latest, VersionAction::MarkDeleted))
                    }
                    _ => (),
                }
            }
            VersionRule::Cleanup {
                keep,
                before,
                orphan_markers,
            } => {
                cleanup_versions(versions, latest, *keep, *before, *orphan_markers, &mut plan);
            }
            _ => (),
        }
    }
    Ok(plan)
}

fn validate_rule(rule: &VersionRule) -> Result<()> {
    if let VersionRule::Cleanup {
        keep,
        before,
        orphan_markers,
    } = rule
        && (*keep == Some(0) || (keep.is_none() && before.is_none() && !orphan_markers))
    {
        return Err(AppError::Invalid(
            "Set a positive keep count, an age cutoff, or orphan marker cleanup.".into(),
        ));
    }
    Ok(())
}

fn cleanup_versions(
    versions: &[&ObjectVersion],
    latest: &ObjectVersion,
    keep: Option<usize>,
    before: Option<DateTime<Utc>>,
    orphan_markers: bool,
    plan: &mut Vec<VersionPlanItem>,
) {
    let first = plan.len();
    let mut retained_data = false;
    let mut data_position = 0;
    let mut markers = Vec::new();
    for row in versions {
        if row.delete_marker {
            if !row.latest {
                markers.push(*row);
            }
            continue;
        }
        let remove = !row.latest
            && (keep.is_some_and(|count| data_position >= count)
                || before.is_some_and(|cutoff| modified(row).is_some_and(|date| date < cutoff)));
        data_position += 1;
        if remove {
            plan.push(plan_item(row, latest, VersionAction::DeleteVersion));
        } else {
            retained_data = true;
        }
    }
    plan[first..].reverse();
    if orphan_markers && !retained_data {
        plan.extend(
            markers
                .into_iter()
                .map(|row| plan_item(row, latest, VersionAction::DeleteVersion)),
        );
    }
}

fn plan_item(
    row: &ObjectVersion,
    latest: &ObjectVersion,
    action: VersionAction,
) -> VersionPlanItem {
    VersionPlanItem {
        key: row.key.clone(),
        version_id: row.version_id.clone(),
        expected_latest: latest.version_id.clone(),
        action,
    }
}

pub async fn apply_version(
    client: &Client,
    location: &Location,
    rule: &VersionRule,
    item: &VersionPlanItem,
) -> Result<()> {
    let exact = Location {
        prefix: item.key.clone(),
        ..location.clone()
    };
    let mut rows = prefix_history(client, &exact).await?;
    rows.retain(|row| row.key == item.key);
    if !version_plan(&rows, rule)?.contains(item) {
        return Err(AppError::Invalid("The version history changed after preview. Review the current history before retrying.".into()));
    }
    let object = ObjectId {
        bucket: location.bucket.clone(),
        key: item.key.clone(),
        version_id: (item.action != VersionAction::MarkDeleted).then(|| item.version_id.clone()),
    };
    match item.action {
        VersionAction::Restore => {
            let restored =
                ObjectService::change_with_version(client, &object, ObjectChange::RestoreVersion)
                    .await?;
            verify_restored(client, &exact, item, restored.as_deref()).await
        }
        VersionAction::DeleteVersion => ObjectService::delete(client, &object).await,
        VersionAction::MarkDeleted => {
            let etag = crate::object::repository::head(
                client,
                &ObjectId {
                    version_id: Some(item.version_id.clone()),
                    ..object.clone()
                },
            )
            .await?;
            let etag = crate::object::repository::required_id(etag.e_tag(), "source ETag")?;
            client
                .delete_object()
                .bucket(&object.bucket)
                .key(&object.key)
                .if_match(etag)
                .send()
                .await
                .map_err(crate::common::error::storage_error)?;
            Ok(())
        }
    }
}

async fn verify_restored(
    client: &Client,
    exact: &Location,
    item: &VersionPlanItem,
    restored: Option<&str>,
) -> Result<()> {
    let versions = prefix_history(client, exact).await?;
    let current = versions
        .iter()
        .find(|row| row.key == item.key && row.latest && !row.delete_marker);
    if current.is_some_and(|row| {
        restored.map_or_else(
            || row.version_id != item.expected_latest,
            |id| row.version_id == id,
        )
    }) {
        return Ok(());
    }
    Err(AppError::Storage(
        "The endpoint did not confirm the restored version as current.".into(),
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/transfer/batch.rs"]
mod tests;
