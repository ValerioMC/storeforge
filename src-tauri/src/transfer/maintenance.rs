use std::collections::HashSet;

use aws_sdk_s3::Client;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::{
    bucket::service::BucketService,
    common::error::{AppError, Result, storage_error},
    connection::service::Session,
    object::model::{Location, ObjectId, ObjectRow, ObjectVersion},
};

use super::{
    inventory,
    security::{transfer_error, write_client},
};

const MAX_PARALLELISM: usize = 32;
const MAX_SAMPLE_BYTES: u64 = 16 * 1024 * 1024;
const FAILURE_LIMIT: usize = 200;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketScope {
    pub bucket: String,
    pub prefix: String,
}

impl BucketScope {
    pub fn label(&self) -> String {
        if self.prefix.is_empty() {
            self.bucket.clone()
        } else {
            format!("{}/{}", self.bucket, self.prefix)
        }
    }

    fn location(&self, connection: Uuid) -> Location {
        Location {
            connection,
            bucket: self.bucket.clone(),
            prefix: self.prefix.clone(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketSelection {
    pub connection: Uuid,
    pub targets: Vec<BucketScope>,
    pub parallelism: usize,
}

impl BucketSelection {
    pub fn validate(&self) -> Result<()> {
        let mut buckets = HashSet::new();
        if self.targets.is_empty()
            || self
                .targets
                .iter()
                .any(|scope| scope.bucket.is_empty() || !buckets.insert(&scope.bucket))
        {
            return Err(AppError::Invalid("Select each bucket once.".into()));
        }
        if !(1..=MAX_PARALLELISM).contains(&self.parallelism) {
            return Err(AppError::Invalid(format!(
                "Parallelism must be between 1 and {MAX_PARALLELISM}."
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IntegrityMethod {
    Head,
    Sample { bytes: u64 },
    Full,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityRequest {
    pub selection: BucketSelection,
    pub method: IntegrityMethod,
    pub since: Option<DateTime<Utc>>,
    pub include_versions: bool,
}

impl IntegrityRequest {
    pub fn validate(&self) -> Result<()> {
        self.selection.validate()?;
        if let IntegrityMethod::Sample { bytes } = self.method
            && !(1..=MAX_SAMPLE_BYTES).contains(&bytes)
        {
            return Err(AppError::Invalid(format!(
                "Sample size must be between 1 and {MAX_SAMPLE_BYTES} bytes."
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeRequest {
    pub selection: BucketSelection,
    pub include_versions: bool,
    pub remove_bucket: bool,
}

impl PurgeRequest {
    fn validate(&self) -> Result<()> {
        self.selection.validate()?;
        if self.remove_bucket
            && (!self.include_versions
                || self
                    .selection
                    .targets
                    .iter()
                    .any(|target| !target.prefix.is_empty()))
        {
            return Err(AppError::Invalid(
                "Deleting a bucket requires its root and all versions.".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketCounts {
    current: usize,
    history: usize,
    markers: usize,
}

impl BucketCounts {
    fn count(&mut self, item: &ScanItem) {
        if item.marker {
            self.markers += 1;
        } else if item.latest {
            self.current += 1;
        } else {
            self.history += 1;
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeTarget {
    pub scope: BucketScope,
    pub counts: Option<BucketCounts>,
    history_error: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgePreview {
    pub request: PurgeRequest,
    pub revision: Option<Uuid>,
    pub buckets: Vec<PurgeTarget>,
    pub confirmation: String,
}

impl PurgePreview {
    pub fn validate(&self, session: &Session, confirmation: &str) -> Result<()> {
        self.request.validate()?;
        if self.request.selection.connection != session.profile.id
            || self.revision != session.profile.revision
            || self.buckets.len() != self.request.selection.targets.len()
            || !self
                .buckets
                .iter()
                .zip(&self.request.selection.targets)
                .all(|(bucket, target)| &bucket.scope == target)
        {
            return Err(AppError::Invalid(
                "The purge scope changed. Preview it again.".into(),
            ));
        }
        let expected = confirmation_phrase(&self.buckets);
        if expected.is_empty() || confirmation != expected || self.confirmation != expected {
            return Err(AppError::Invalid(
                "Confirm the exact reviewed purge targets.".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MaintenanceOptions {
    Integrity {
        parallelism: usize,
        method: IntegrityMethod,
        since: Option<DateTime<Utc>>,
        include_versions: bool,
    },
    Purge {
        parallelism: usize,
        include_versions: bool,
        remove_bucket: bool,
    },
}

impl From<&IntegrityRequest> for MaintenanceOptions {
    fn from(request: &IntegrityRequest) -> Self {
        Self::Integrity {
            parallelism: request.selection.parallelism,
            method: request.method,
            since: request.since,
            include_versions: request.include_versions,
        }
    }
}

impl From<&PurgeRequest> for MaintenanceOptions {
    fn from(request: &PurgeRequest) -> Self {
        Self::Purge {
            parallelism: request.selection.parallelism,
            include_versions: request.include_versions,
            remove_bucket: request.remove_bucket,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BucketStatus {
    Pending,
    Listing,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum FailureStage {
    Listing,
    Review,
    Filter,
    Head,
    Get,
    Delete,
    DeleteBucket,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct MaintenanceFailure {
    stage: FailureStage,
    key: Option<String>,
    version: Option<String>,
    message: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketResult {
    pub scope: BucketScope,
    pub status: BucketStatus,
    planned: BucketCounts,
    completed: BucketCounts,
    pub failed: usize,
    failures: Vec<MaintenanceFailure>,
    removed: bool,
}

impl BucketResult {
    pub fn new(scope: BucketScope) -> Self {
        Self {
            scope,
            status: BucketStatus::Pending,
            planned: BucketCounts::default(),
            completed: BucketCounts::default(),
            failed: 0,
            failures: Vec::new(),
            removed: false,
        }
    }

    fn failure(&mut self, stage: FailureStage, item: Option<&ScanItem>, error: AppError) {
        self.failed += 1;
        if self.failures.len() < FAILURE_LIMIT {
            self.failures.push(MaintenanceFailure {
                stage,
                key: item.map(|item| item.object.key.clone()),
                version: item.and_then(|item| item.object.version_id.clone()),
                message: error.to_string(),
            });
        }
    }

    fn finish(&mut self, proceed: bool) {
        self.status = if !proceed {
            BucketStatus::Cancelled
        } else if self.failed > 0 {
            BucketStatus::Failed
        } else {
            BucketStatus::Completed
        };
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceReport {
    pub connection: Uuid,
    #[serde(flatten)]
    pub options: MaintenanceOptions,
    pub buckets: Vec<BucketResult>,
}

struct ScanItem {
    object: ObjectId,
    latest: bool,
    marker: bool,
    size: Option<i64>,
    etag: Option<String>,
    modified: Option<String>,
}

impl ScanItem {
    fn current(bucket: &str, row: ObjectRow) -> Self {
        Self {
            object: ObjectId {
                bucket: bucket.into(),
                key: row.key,
                version_id: None,
            },
            latest: true,
            marker: false,
            size: row.size,
            etag: row.etag,
            modified: row.modified,
        }
    }

    fn version(bucket: &str, row: ObjectVersion) -> Self {
        Self {
            object: ObjectId {
                bucket: bucket.into(),
                key: row.key,
                version_id: Some(row.version_id),
            },
            latest: row.latest,
            marker: row.delete_marker,
            size: row.size,
            etag: None,
            modified: row.modified,
        }
    }
}

pub async fn preview_purge(session: &Session, request: PurgeRequest) -> Result<PurgePreview> {
    request.validate()?;
    let mut buckets = Vec::with_capacity(request.selection.targets.len());
    for scope in &request.selection.targets {
        let location = scope.location(session.profile.id);
        let mut preview = PurgeTarget {
            scope: scope.clone(),
            counts: None,
            history_error: None,
            error: None,
        };
        match scan_items(&session.s3, &location, request.include_versions).await {
            Ok(items) => {
                let mut counts = count_items(&items);
                if !request.include_versions {
                    match scan_items(&session.s3, &location, true).await {
                        Ok(versions) => {
                            let version_counts = count_items(&versions);
                            counts.history = version_counts.history;
                            counts.markers = version_counts.markers;
                        }
                        Err(error) => preview.history_error = Some(error.to_string()),
                    }
                }
                preview.counts = Some(counts);
            }
            Err(error) => preview.error = Some(error.to_string()),
        }
        buckets.push(preview);
    }
    let confirmation = confirmation_phrase(&buckets);
    Ok(PurgePreview {
        request,
        revision: session.profile.revision,
        buckets,
        confirmation,
    })
}

fn confirmation_phrase(targets: &[PurgeTarget]) -> String {
    targets
        .iter()
        .filter(|target| target.counts.is_some())
        .map(|target| target.scope.label())
        .collect::<Vec<_>>()
        .join(", ")
}

async fn scan_items(
    client: &Client,
    location: &Location,
    include_versions: bool,
) -> Result<Vec<ScanItem>> {
    let mut items = Vec::new();
    if include_versions {
        inventory::visit_versions(client, location, |page| {
            items.extend(
                page.into_iter()
                    .map(|row| ScanItem::version(&location.bucket, row)),
            );
        })
        .await?;
    } else {
        inventory::visit_objects(client, location, |page| {
            items.extend(
                page.into_iter()
                    .filter(|row| !row.folder)
                    .map(|row| ScanItem::current(&location.bucket, row)),
            );
        })
        .await?;
    }
    if items
        .iter()
        .any(|item| !item.object.key.starts_with(&location.prefix))
    {
        return Err(AppError::Storage(
            "The listing returned keys outside the reviewed prefix.".into(),
        ));
    }
    Ok(items)
}

fn count_items(items: &[ScanItem]) -> BucketCounts {
    let mut counts = BucketCounts::default();
    for item in items {
        counts.count(item);
    }
    counts
}

pub async fn run_integrity(
    session: &Session,
    scope: BucketScope,
    request: &IntegrityRequest,
    publish: impl Fn(&BucketResult) -> Result<bool>,
) -> Result<BucketResult> {
    let mut result = BucketResult::new(scope);
    result.status = BucketStatus::Listing;
    if !publish(&result)? {
        result.finish(false);
        return Ok(result);
    }
    let location = result.scope.location(session.profile.id);
    match scan_items(&session.s3, &location, request.include_versions).await {
        Ok(mut items) => {
            items.retain(|item| check_scope(&mut result, item, request.since));
            result.planned = count_items(&items);
            result.status = BucketStatus::Running;
            let proceed = run_items(
                &session.s3,
                items,
                request.selection.parallelism,
                ScanAction::Integrity(request.method),
                &mut result,
                &publish,
            )
            .await?;
            result.finish(proceed);
        }
        Err(error) => {
            result.failure(FailureStage::Listing, None, error);
            result.finish(true);
        }
    }
    Ok(result)
}

fn check_scope(result: &mut BucketResult, item: &ScanItem, since: Option<DateTime<Utc>>) -> bool {
    if item.marker {
        return false;
    }
    let Some(since) = since else { return true };
    let modified = item.modified.as_deref().and_then(inventory::parse_modified);
    match modified {
        Some(modified) => modified >= since,
        None => {
            result.failure(
                FailureStage::Filter,
                Some(item),
                AppError::Storage("The provider omitted a valid modification time.".into()),
            );
            false
        }
    }
}

pub async fn run_purge(
    session: &Session,
    target: &PurgeTarget,
    request: &PurgeRequest,
    publish: impl Fn(&BucketResult) -> Result<bool>,
) -> Result<BucketResult> {
    let mut result = BucketResult::new(target.scope.clone());
    let Some(reviewed) = target.counts else {
        let message = target
            .error
            .clone()
            .unwrap_or_else(|| "The bucket was unavailable in the preview.".into());
        result.failure(FailureStage::Listing, None, AppError::Storage(message));
        result.finish(true);
        return Ok(result);
    };
    result.status = BucketStatus::Listing;
    if !publish(&result)? {
        result.finish(false);
        return Ok(result);
    }
    let location = result.scope.location(session.profile.id);
    let items = match scan_items(&session.s3, &location, request.include_versions).await {
        Ok(items) => items,
        Err(error) => {
            result.failure(FailureStage::Listing, None, error);
            result.finish(true);
            return Ok(result);
        }
    };
    result.planned = count_items(&items);
    if result.planned.current != reviewed.current
        || request.include_versions && result.planned != reviewed
    {
        result.failure(
            FailureStage::Review,
            None,
            AppError::Invalid("The bucket inventory changed. Preview this bucket again.".into()),
        );
        result.finish(true);
        return Ok(result);
    }
    result.status = BucketStatus::Running;
    let (history, current): (Vec<_>, Vec<_>) = items.into_iter().partition(|item| !item.latest);
    let client = write_client(&session.s3);
    let parallelism = request.selection.parallelism;
    let mut proceed = run_items(
        &client,
        history,
        parallelism,
        ScanAction::Purge,
        &mut result,
        &publish,
    )
    .await?;
    if proceed && result.failed == 0 {
        proceed = run_items(
            &client,
            current,
            parallelism,
            ScanAction::Purge,
            &mut result,
            &publish,
        )
        .await?;
    }
    if proceed && result.failed == 0 && request.remove_bucket {
        proceed = publish(&result)?;
        if proceed {
            match BucketService::delete(session, &location.bucket).await {
                Ok(()) => result.removed = true,
                Err(error) => result.failure(FailureStage::DeleteBucket, None, error),
            }
        }
    }
    result.finish(proceed);
    Ok(result)
}

#[derive(Clone, Copy)]
enum ScanAction {
    Integrity(IntegrityMethod),
    Purge,
}

impl ScanAction {
    fn stage(self) -> FailureStage {
        match self {
            Self::Integrity(IntegrityMethod::Head) => FailureStage::Head,
            Self::Integrity(_) => FailureStage::Get,
            Self::Purge => FailureStage::Delete,
        }
    }

    async fn run(self, client: &Client, item: &ScanItem) -> Result<()> {
        match self {
            Self::Integrity(method) => check_item(client, item, method).await,
            Self::Purge => delete_item(client, item).await,
        }
    }
}

async fn run_items(
    client: &Client,
    items: Vec<ScanItem>,
    parallelism: usize,
    action: ScanAction,
    result: &mut BucketResult,
    publish: &impl Fn(&BucketResult) -> Result<bool>,
) -> Result<bool> {
    let mut items = items.into_iter();
    let mut tasks = JoinSet::new();
    let mut proceed = publish(result)?;
    loop {
        while proceed && tasks.len() < parallelism {
            let Some(item) = items.next() else { break };
            let client = client.clone();
            tasks.spawn(async move {
                let outcome = action.run(&client, &item).await;
                (item, outcome)
            });
        }
        let Some(task) = tasks.join_next().await else {
            return Ok(proceed);
        };
        let (item, outcome) =
            task.map_err(|error| AppError::Storage(format!("Bucket worker stopped: {error}")))?;
        match outcome {
            Ok(()) => result.completed.count(&item),
            Err(error) => result.failure(action.stage(), Some(&item), error),
        }
        proceed &= publish(result)?;
    }
}

async fn delete_item(client: &Client, item: &ScanItem) -> Result<()> {
    let object = &item.object;
    client
        .delete_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .set_if_match(item.etag.clone())
        .send()
        .await
        .map_err(|error| transfer_error(error, false, true))?;
    Ok(())
}

async fn check_item(client: &Client, item: &ScanItem, method: IntegrityMethod) -> Result<()> {
    let size = item
        .size
        .and_then(|size| u64::try_from(size).ok())
        .ok_or_else(|| AppError::Storage("The provider omitted a valid listed size.".into()))?;
    match method {
        IntegrityMethod::Head => check_head(client, item, size).await,
        IntegrityMethod::Sample { bytes } if bytes < size => {
            check_body(client, item, size, Some(bytes)).await
        }
        IntegrityMethod::Sample { .. } | IntegrityMethod::Full => {
            check_body(client, item, size, None).await
        }
    }
}

async fn check_head(client: &Client, item: &ScanItem, size: u64) -> Result<()> {
    let object = &item.object;
    let head = client
        .head_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .set_if_match(item.etag.clone())
        .send()
        .await
        .map_err(storage_error)?;
    require_length(head.content_length(), size)
}

async fn check_body(
    client: &Client,
    item: &ScanItem,
    size: u64,
    sample: Option<u64>,
) -> Result<()> {
    let object = &item.object;
    let range = sample.map(|bytes| format!("bytes=0-{}", bytes - 1));
    let mut response = client
        .get_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .set_if_match(item.etag.clone())
        .set_range(range)
        .send()
        .await
        .map_err(storage_error)?;
    let expected = sample.unwrap_or(size);
    require_length(response.content_length(), expected)?;
    if let Some(sample) = sample
        && response.content_range() != Some(format!("bytes 0-{}/{size}", sample - 1).as_str())
    {
        return Err(AppError::Storage(
            "The provider did not return the requested byte range.".into(),
        ));
    }
    let mut read = 0u64;
    while let Some(part) = response.body.next().await {
        let part =
            part.map_err(|error| AppError::Storage(format!("Object read failed: {error}")))?;
        read = read.saturating_add(part.len() as u64);
        if read > expected {
            return Err(AppError::Storage(
                "The object body exceeded the requested length.".into(),
            ));
        }
    }
    if read != expected {
        return Err(AppError::Storage(
            "The object body ended before the requested length.".into(),
        ));
    }
    Ok(())
}

fn require_length(length: Option<i64>, expected: u64) -> Result<()> {
    if length.and_then(|value| u64::try_from(value).ok()) != Some(expected) {
        return Err(AppError::Storage(
            "The provider reported a different byte length than expected.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/transfer/integrity_purge.rs"]
mod integrity_purge;
