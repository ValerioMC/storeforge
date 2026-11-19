use std::collections::{BTreeMap, BTreeSet};

use aws_sdk_s3::{
    Client,
    operation::head_object::HeadObjectOutput,
    types::{ChecksumMode, ChecksumType},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{
    bucket::{
        model::{Configuration, ConfigurationChange},
        repository as buckets,
    },
    common::error::{AppError, Result, storage_error},
    connection::service::Session,
    object::{
        model::{Location, ObjectId, ObjectRow},
        repository as objects,
    },
};

use super::{
    files::{self, UploadOptions},
    inventory,
    security::write_client,
};

const DETAIL_LIMIT: usize = 200;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketPair {
    pub source: String,
    pub target: String,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentMode {
    None,
    Size,
    Etag,
    Checksum,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonRequest {
    pub pairs: Vec<BucketPair>,
    pub content: ContentMode,
    pub modified_before: Option<DateTime<Utc>>,
    pub configuration: Vec<Configuration>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonScope {
    source_connection: Uuid,
    target_connection: Uuid,
    source_revision: Option<Uuid>,
    target_revision: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketComparison {
    scope: ComparisonScope,
    reports: Vec<PairReport>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairReport {
    pair: BucketPair,
    contents: Option<ContentReport>,
    content_error: Option<String>,
    configuration: Vec<ConfigurationReport>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentReport {
    source_count: usize,
    target_count: usize,
    matching_size: usize,
    matching_digest: usize,
    different_size: usize,
    different_digest: usize,
    inconclusive: usize,
    source_only: usize,
    target_only: usize,
    after_cutoff: usize,
    unknown_modified: usize,
    hidden: usize,
    details: Vec<ObjectDifference>,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
pub struct ObjectSnapshot {
    size: Option<i64>,
    etag: Option<String>,
    modified: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Evidence {
    SourceOnly,
    TargetOnly,
    DifferentSize,
    DifferentDigest,
    MatchingSize,
    MatchingDigest,
    Inconclusive,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObjectDifference {
    key: String,
    evidence: Evidence,
    source: Option<ObjectSnapshot>,
    target: Option<ObjectSnapshot>,
    matching_etag: Option<bool>,
    issue: Option<String>,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(tag = "status", content = "value", rename_all = "camelCase")]
pub enum ConfigurationSnapshot {
    Configured(String),
    Unset,
    Error(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigurationReport {
    configuration: Configuration,
    source: ConfigurationSnapshot,
    target: ConfigurationSnapshot,
    different: bool,
    can_apply: bool,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Remediation {
    Object {
        key: String,
        source: Option<ObjectSnapshot>,
        target: Option<ObjectSnapshot>,
    },
    Configuration {
        configuration: Configuration,
        source: ConfigurationSnapshot,
        target: ConfigurationSnapshot,
    },
}

impl ComparisonScope {
    fn new(source: &Session, target: &Session) -> Self {
        Self {
            source_connection: source.profile.id,
            target_connection: target.profile.id,
            source_revision: source.profile.revision,
            target_revision: target.profile.revision,
        }
    }

    pub fn connections(&self) -> (Uuid, Uuid) {
        (self.source_connection, self.target_connection)
    }

    fn validate(&self, source: &Session, target: &Session) -> Result<()> {
        let current = Self::new(source, target);
        if self.connections() != current.connections()
            || self.source_revision != current.source_revision
            || self.target_revision != current.target_revision
        {
            return Err(AppError::Invalid(
                "A connection changed. Run the comparison again.".into(),
            ));
        }
        Ok(())
    }
}

impl BucketPair {
    fn locations(&self, source: &Session, target: &Session) -> Result<(Location, Location)> {
        if self.source.is_empty() || self.target.is_empty() {
            return Err(AppError::Invalid(
                "Select both buckets in every mapping.".into(),
            ));
        }
        if self.source == self.target
            && source.context.endpoint_id == target.context.endpoint_id
            && source.context.tenant == target.context.tenant
        {
            return Err(AppError::Invalid("Select two different buckets.".into()));
        }
        Ok((
            bucket_location(source, &self.source),
            bucket_location(target, &self.target),
        ))
    }
}

fn bucket_location(session: &Session, bucket: &str) -> Location {
    Location {
        connection: session.profile.id,
        bucket: bucket.into(),
        prefix: String::new(),
    }
}

pub async fn compare(
    source: &Session,
    target: &Session,
    request: ComparisonRequest,
) -> Result<BucketComparison> {
    if request.pairs.is_empty()
        || (matches!(request.content, ContentMode::None) && request.configuration.is_empty())
    {
        return Err(AppError::Invalid(
            "Choose bucket mappings and at least one comparison.".into(),
        ));
    }
    let mut destinations = BTreeSet::new();
    let mut configurations = BTreeSet::new();
    if request
        .pairs
        .iter()
        .any(|pair| !destinations.insert(&pair.target))
        || request
            .configuration
            .iter()
            .any(|feature| !configurations.insert(*feature))
    {
        return Err(AppError::Invalid(
            "Use each destination and configuration section only once.".into(),
        ));
    }
    let locations = request
        .pairs
        .iter()
        .map(|pair| pair.locations(source, target))
        .collect::<Result<Vec<_>>>()?;
    let mut reports = Vec::with_capacity(request.pairs.len());
    for (pair, (source_location, target_location)) in request.pairs.into_iter().zip(locations) {
        let mut report = PairReport {
            pair,
            contents: None,
            content_error: None,
            configuration: Vec::new(),
        };
        if !matches!(request.content, ContentMode::None) {
            match compare_contents(
                (&source.s3, &source_location),
                (&target.s3, &target_location),
                request.content,
                request.modified_before,
            )
            .await
            {
                Ok(contents) => report.contents = Some(contents),
                Err(error) => report.content_error = Some(error.to_string()),
            }
        }
        for &configuration in &request.configuration {
            report.configuration.push(
                compare_configuration(
                    (source, &source_location.bucket),
                    (target, &target_location.bucket),
                    configuration,
                )
                .await,
            );
        }
        reports.push(report);
    }
    Ok(BucketComparison {
        scope: ComparisonScope::new(source, target),
        reports,
    })
}

async fn compare_contents(
    source: (&Client, &Location),
    target: (&Client, &Location),
    mode: ContentMode,
    cutoff: Option<DateTime<Utc>>,
) -> Result<ContentReport> {
    let (source_rows, target_rows) = tokio::try_join!(
        inventory::objects(source.0, source.1),
        inventory::objects(target.0, target.1)
    )?;
    let source_rows = object_index(&source_rows);
    let target_rows = object_index(&target_rows);
    let keys: BTreeSet<_> = source_rows
        .keys()
        .chain(target_rows.keys())
        .copied()
        .collect();
    let mut report = ContentReport::default();
    for key in keys {
        let left = source_rows.get(key).copied();
        let right = target_rows.get(key).copied();
        match cutoff_status(left, right, cutoff) {
            CutoffStatus::Included => {}
            CutoffStatus::After => {
                report.after_cutoff += 1;
                continue;
            }
            CutoffStatus::Unknown => {
                report.unknown_modified += 1;
                continue;
            }
        }
        report.source_count += usize::from(left.is_some());
        report.target_count += usize::from(right.is_some());
        let (evidence, issue) = object_evidence(source, target, left, right, mode).await;
        report.record(key, evidence, left, right, issue);
    }
    Ok(report)
}

fn object_index(rows: &[ObjectRow]) -> BTreeMap<&str, &ObjectRow> {
    rows.iter()
        .filter(|row| !row.folder)
        .map(|row| (row.key.as_str(), row))
        .collect()
}

enum CutoffStatus {
    Included,
    After,
    Unknown,
}

fn cutoff_status(
    left: Option<&ObjectRow>,
    right: Option<&ObjectRow>,
    cutoff: Option<DateTime<Utc>>,
) -> CutoffStatus {
    let Some(cutoff) = cutoff else {
        return CutoffStatus::Included;
    };
    let mut unknown = false;
    for row in left.into_iter().chain(right) {
        match row.modified.as_deref().and_then(inventory::parse_modified) {
            Some(date) if date > cutoff => return CutoffStatus::After,
            Some(_) => {}
            None => unknown = true,
        }
    }
    if unknown {
        CutoffStatus::Unknown
    } else {
        CutoffStatus::Included
    }
}

async fn object_evidence(
    source: (&Client, &Location),
    target: (&Client, &Location),
    left: Option<&ObjectRow>,
    right: Option<&ObjectRow>,
    mode: ContentMode,
) -> (Evidence, Option<String>) {
    let (left, right) = match (left, right) {
        (Some(left), Some(right)) => (left, right),
        (Some(_), None) => return (Evidence::SourceOnly, None),
        (None, Some(_)) => return (Evidence::TargetOnly, None),
        (None, None) => return (Evidence::Inconclusive, None),
    };
    match (left.size, right.size) {
        (Some(a), Some(b)) if a != b => return (Evidence::DifferentSize, None),
        (Some(_), Some(_)) if matches!(mode, ContentMode::Size) => {
            return (Evidence::MatchingSize, None);
        }
        _ => {}
    }
    if matches!(mode, ContentMode::Checksum) {
        let (a, b) = tokio::join!(content_digest(source, left), content_digest(target, right));
        return match (a, b) {
            (Ok(Some(a)), Ok(Some(b))) if a == b => (Evidence::MatchingDigest, None),
            (Ok(Some(_)), Ok(Some(_))) => (Evidence::DifferentDigest, None),
            (Err(error), _) => (
                Evidence::Inconclusive,
                Some(format!("Source checksum: {error}")),
            ),
            (_, Err(error)) => (
                Evidence::Inconclusive,
                Some(format!("Destination checksum: {error}")),
            ),
            _ => (Evidence::Inconclusive, None),
        };
    }
    (Evidence::Inconclusive, None)
}

async fn content_digest(
    (client, location): (&Client, &Location),
    row: &ObjectRow,
) -> Result<Option<String>> {
    let snapshot = ObjectSnapshot::from(row);
    if snapshot.etag.is_none() {
        return Ok(None);
    }
    let result = client
        .head_object()
        .bucket(&location.bucket)
        .key(&row.key)
        .set_if_match(row.etag.clone())
        .checksum_mode(ChecksumMode::Enabled)
        .send()
        .await
        .map_err(storage_error)?;
    if !snapshot.matches_head(&result) {
        return Err(stale_review());
    }
    Ok(full_object_digest(&result))
}

fn full_object_digest(head: &HeadObjectOutput) -> Option<String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let value = head.checksum_sha256()?;
    (head.checksum_type() != Some(&ChecksumType::Composite)
        && STANDARD
            .decode(value)
            .is_ok_and(|digest| digest.len() == 32))
    .then(|| value.to_owned())
}

impl From<&ObjectRow> for ObjectSnapshot {
    fn from(row: &ObjectRow) -> Self {
        Self {
            size: row.size,
            etag: row.etag.clone(),
            modified: row.modified.clone(),
        }
    }
}

impl ObjectSnapshot {
    fn matches_head(&self, head: &HeadObjectOutput) -> bool {
        self.etag.is_some()
            && self.size.is_some()
            && self.size == head.content_length()
            && self.etag.as_deref() == head.e_tag()
            && self.modified == head.last_modified().map(ToString::to_string)
    }
}

impl ContentReport {
    fn record(
        &mut self,
        key: &str,
        evidence: Evidence,
        source: Option<&ObjectRow>,
        target: Option<&ObjectRow>,
        issue: Option<String>,
    ) {
        match evidence {
            Evidence::SourceOnly => self.source_only += 1,
            Evidence::TargetOnly => self.target_only += 1,
            Evidence::DifferentSize => self.different_size += 1,
            Evidence::DifferentDigest => self.different_digest += 1,
            Evidence::MatchingSize => {
                self.matching_size += 1;
                return;
            }
            Evidence::MatchingDigest => {
                self.matching_digest += 1;
                return;
            }
            Evidence::Inconclusive => self.inconclusive += 1,
        }
        if self.details.len() >= DETAIL_LIMIT {
            self.hidden += 1;
            return;
        }
        let matching_etag = source
            .and_then(|row| row.etag.as_ref())
            .zip(target.and_then(|row| row.etag.as_ref()))
            .map(|(left, right)| left == right);
        self.details.push(ObjectDifference {
            key: key.into(),
            evidence,
            source: source.map(ObjectSnapshot::from),
            target: target.map(ObjectSnapshot::from),
            matching_etag,
            issue,
        });
    }
}

async fn read_configuration(
    (session, bucket): (&Session, &str),
    configuration: Configuration,
) -> ConfigurationSnapshot {
    match buckets::configure(session, bucket, configuration, ConfigurationChange::Read).await {
        Ok(document) => ConfigurationSnapshot::Configured(document),
        Err(AppError::Missing(_)) => ConfigurationSnapshot::Unset,
        Err(error) => ConfigurationSnapshot::Error(error.to_string()),
    }
}

async fn compare_configuration(
    source: (&Session, &str),
    target: (&Session, &str),
    configuration: Configuration,
) -> ConfigurationReport {
    let (source, target) = tokio::join!(
        read_configuration(source, configuration),
        read_configuration(target, configuration)
    );
    let readable = !matches!(source, ConfigurationSnapshot::Error(_))
        && !matches!(target, ConfigurationSnapshot::Error(_));
    let different = readable && !same_configuration(configuration, &source, &target);
    let can_apply = different && configuration_change(configuration, &source).is_ok();
    ConfigurationReport {
        configuration,
        source,
        target,
        different,
        can_apply,
    }
}

fn same_configuration(
    configuration: Configuration,
    source: &ConfigurationSnapshot,
    target: &ConfigurationSnapshot,
) -> bool {
    if configuration == Configuration::Policy
        && let (ConfigurationSnapshot::Configured(left), ConfigurationSnapshot::Configured(right)) =
            (source, target)
        && let (Ok(left), Ok(right)) = (
            serde_json::from_str::<serde_json::Value>(left),
            serde_json::from_str::<serde_json::Value>(right),
        )
    {
        return left == right;
    }
    source == target
}

fn configuration_change(
    configuration: Configuration,
    source: &ConfigurationSnapshot,
) -> Result<ConfigurationChange> {
    match source {
        ConfigurationSnapshot::Configured(document) if !document.trim().is_empty() => {
            Ok(ConfigurationChange::Write(document.clone()))
        }
        ConfigurationSnapshot::Unset if buckets::configuration_removable(configuration) => {
            Ok(ConfigurationChange::Remove)
        }
        _ => Err(AppError::Invalid(
            "The source setting cannot be applied to the destination.".into(),
        )),
    }
}

pub async fn apply(
    source: &Session,
    target: &Session,
    scope: ComparisonScope,
    pair: BucketPair,
    action: Remediation,
) -> Result<()> {
    scope.validate(source, target)?;
    pair.locations(source, target)?;
    match action {
        Remediation::Object {
            key,
            source: left,
            target: right,
        } => {
            apply_object(
                (&source.s3, &pair.source),
                (&target.s3, &pair.target),
                key,
                left,
                right,
            )
            .await
        }
        Remediation::Configuration {
            configuration,
            source: left,
            target: right,
        } => {
            apply_configuration(
                (source, &pair.source),
                (target, &pair.target),
                configuration,
                left,
                right,
            )
            .await
        }
    }
}

async fn apply_configuration(
    source: (&Session, &str),
    target: (&Session, &str),
    configuration: Configuration,
    left: ConfigurationSnapshot,
    right: ConfigurationSnapshot,
) -> Result<()> {
    let current = compare_configuration(source, target, configuration).await;
    if !current.can_apply || current.source != left || current.target != right {
        return Err(stale_review());
    }
    let change = configuration_change(configuration, &left)?;
    buckets::configure(target.0, target.1, configuration, change).await?;
    let result = read_configuration(target, configuration).await;
    if !same_configuration(configuration, &left, &result) {
        return Err(AppError::Storage(
            "The provider did not verify the destination configuration. Run the comparison again."
                .into(),
        ));
    }
    Ok(())
}

fn stale_review() -> AppError {
    AppError::Invalid(
        "The reviewed result changed or could not be verified. Run the comparison again.".into(),
    )
}

async fn reviewed_head(
    client: &Client,
    object: &ObjectId,
    expected: Option<&ObjectSnapshot>,
) -> Result<Option<HeadObjectOutput>> {
    let result = client
        .head_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_if_match(expected.and_then(|snapshot| snapshot.etag.clone()))
        .send()
        .await;
    match (result, expected) {
        (Ok(head), Some(snapshot)) if snapshot.matches_head(&head) => Ok(Some(head)),
        (Err(error), None)
            if error
                .raw_response()
                .is_some_and(|response| response.status().as_u16() == 404) =>
        {
            Ok(None)
        }
        (Err(error), _)
            if !error
                .raw_response()
                .is_some_and(|response| matches!(response.status().as_u16(), 404 | 412)) =>
        {
            Err(storage_error(error))
        }
        _ => Err(stale_review()),
    }
}

async fn apply_object(
    source: (&Client, &str),
    target: (&Client, &str),
    key: String,
    left: Option<ObjectSnapshot>,
    right: Option<ObjectSnapshot>,
) -> Result<()> {
    if key.is_empty() || (left.is_none() && right.is_none()) {
        return Err(stale_review());
    }
    let mut source_object = ObjectId {
        bucket: source.1.into(),
        key: key.clone(),
        version_id: None,
    };
    let mut target_object = ObjectId {
        bucket: target.1.into(),
        key,
        version_id: None,
    };
    let (source_head, target_head) = tokio::try_join!(
        reviewed_head(source.0, &source_object, left.as_ref()),
        reviewed_head(target.0, &target_object, right.as_ref())
    )?;
    let target_etag = target_head.and_then(|head| head.e_tag);
    if let Some(head) = source_head {
        source_object.version_id = head.version_id;
        let mut file = NamedTempFile::new()?;
        let size = files::stream_to_file(
            source.0,
            &source_object,
            &mut file,
            head.e_tag,
            Default::default(),
            &|_| Ok(()),
        )
        .await?;
        if head.content_length != i64::try_from(size).ok() {
            return Err(stale_review());
        }
        let receipt = files::upload(
            target.0,
            target.1,
            &target_object.key,
            file.path(),
            UploadOptions {
                content_type: head
                    .content_type
                    .unwrap_or_else(|| "application/octet-stream".into()),
                overwrite: target_etag.is_some(),
                expected_etag: target_etag,
                metadata: head.metadata,
                cache_control: head.cache_control,
                content_disposition: head.content_disposition,
                content_encoding: head.content_encoding,
                content_language: head.content_language,
                ..Default::default()
            },
        )
        .await?;
        target_object.version_id = receipt.version_id;
        let head = objects::head(target.0, &target_object).await?;
        if head.content_length() != i64::try_from(size).ok()
            || receipt.etag.is_none()
            || head.e_tag() != receipt.etag.as_deref()
        {
            return Err(AppError::Storage(
                "The destination could not be verified after copying.".into(),
            ));
        }
    } else {
        let etag = target_etag.ok_or_else(stale_review)?;
        write_client(target.0)
            .delete_object()
            .bucket(target.1)
            .key(&target_object.key)
            .if_match(etag)
            .send()
            .await
            .map_err(|error| super::security::transfer_error(error, false, true))?;
        reviewed_head(target.0, &target_object, None).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/transfer/bucket_comparison.rs"]
mod tests;
