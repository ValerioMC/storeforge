use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use aws_sdk_s3::{
    Client,
    operation::head_object::HeadObjectOutput,
    types::{CompletedMultipartUpload, CompletedPart},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use tokio::task::JoinSet;

use crate::{
    common::error::{AppError, Result},
    connection::service::Session,
    object::{model::ObjectId, repository as objects},
    transfer::{
        files::{self, UploadOptions},
        security::{transfer_error, write_client},
    },
};

use super::{
    execution_model::Fingerprint,
    history::{self, Entry, Inventory},
    model::{Mapping, Method},
    preflight::exact_tags,
};

pub type Evidence = BTreeMap<String, Vec<Fingerprint>>;
pub type VersionEvidence = BTreeMap<String, Vec<(String, Fingerprint)>>;

pub struct Replay<'a> {
    pub source: &'a Session,
    pub target: &'a Session,
    pub mapping: &'a Mapping,
    pub history: bool,
    pub method: Method,
    pub parallelism: usize,
}

pub async fn sync(
    replay: Replay<'_>,
    baseline: &Evidence,
    mut proceed: impl FnMut(usize, usize, Option<&str>) -> Result<bool>,
) -> Result<bool> {
    let mut source_entries =
        history::inventory(replay.source, &replay.mapping.source, replay.history).await?;
    let mut target_entries =
        history::inventory(replay.target, &replay.mapping.target, replay.history).await?;
    let keys: BTreeSet<_> = source_entries
        .keys()
        .chain(target_entries.keys())
        .cloned()
        .collect();
    let total = keys.len();
    let mut completed = 0;
    let clients = Arc::new((replay.source.s3.clone(), replay.target.s3.clone()));
    let target_bucket = Arc::new(replay.mapping.target.clone());
    let mut keys = keys.into_iter().peekable();
    if !proceed(0, total, keys.peek().map(String::as_str))? {
        return Ok(false);
    }
    while let Some(first) = keys.next() {
        let mut tasks = JoinSet::new();
        for key in std::iter::once(first).chain(keys.by_ref().take(replay.parallelism - 1)) {
            let source = source_entries.remove(&key).unwrap_or_default();
            let target = target_entries.remove(&key).unwrap_or_default();
            let previous = baseline.get(&key).cloned().unwrap_or_default();
            let clients = Arc::clone(&clients);
            let bucket = Arc::clone(&target_bucket);
            let method = replay.method;
            tasks.spawn(async move {
                sync_key(&clients, &bucket, &key, method, &source, &target, &previous)
                    .await
                    .map_err(|error| AppError::Storage(format!("Key {key:?} · {error}")))
            });
        }
        let mut failure = None;
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Ok(())) => completed += 1,
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(_) => {
                    failure.get_or_insert(worker_failed());
                }
            }
        }
        let running = proceed(completed, total, keys.peek().map(String::as_str))?;
        if let Some(error) = failure {
            return Err(error);
        }
        if !running {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn sync_key(
    clients: &(Client, Client),
    bucket: &str,
    key: &str,
    method: Method,
    source: &[Entry],
    target: &[Entry],
    previous: &[Fingerprint],
) -> Result<()> {
    let source_evidence = evidence_for_key(&clients.0, source).await?;
    let target_evidence = evidence_for_key(&clients.1, target).await?;
    let common = reviewed_prefix(&source_evidence, &target_evidence, previous)?;
    for (entry, expected) in target[common..]
        .iter()
        .zip(&target_evidence[common..])
        .rev()
    {
        if fingerprint(&clients.1, entry).await? != *expected {
            return Err(divergence());
        }
        remove_entry(&clients.1, entry).await?;
    }
    for (entry, expected) in source[common..].iter().zip(&source_evidence[common..]) {
        let destination = if entry.marker {
            create_marker(&clients.1, bucket, key).await?
        } else {
            copy_entry(clients, bucket, entry, expected, method).await?
        };
        if fingerprint(&clients.1, &destination).await? != *expected {
            return Err(AppError::Storage(
                "The copied content, tags, or delete marker did not verify.".into(),
            ));
        }
    }
    Ok(())
}

pub fn reviewed_prefix(
    source: &[Fingerprint],
    target: &[Fingerprint],
    prior: &[Fingerprint],
) -> Result<usize> {
    let prefix = source
        .iter()
        .zip(target)
        .take_while(|(left, right)| left == right)
        .count();
    if prefix < target.len() && prior.get(prefix..target.len()) != Some(&target[prefix..]) {
        return Err(divergence());
    }
    Ok(prefix)
}

async fn copy_entry(
    clients: &(Client, Client),
    bucket: &str,
    entry: &Entry,
    expected: &Fingerprint,
    method: Method,
) -> Result<Entry> {
    let Fingerprint::Data { tags, .. } = expected else {
        return Err(worker_failed());
    };
    let object = &entry.object;
    let head = objects::head(&clients.0, object).await?;
    let etag = objects::required_id(head.e_tag(), "source ETag")?;
    let version = if method == Method::Storage {
        copy_in_storage(&clients.1, bucket, object, &head, &etag, tags).await?
    } else {
        let mut file = NamedTempFile::new()?;
        let bytes = files::stream_to_file(
            &clients.0,
            object,
            &mut file,
            Some(etag),
            Default::default(),
            &|_| Ok(()),
        )
        .await?;
        if head.content_length != i64::try_from(bytes).ok() {
            return Err(AppError::Storage(
                "The source changed during the streamed copy.".into(),
            ));
        }
        files::upload(
            &clients.1,
            bucket,
            &object.key,
            file.path(),
            UploadOptions {
                overwrite: object.version_id.is_some(),
                content_type: head
                    .content_type
                    .unwrap_or_else(|| "application/octet-stream".into()),
                tagging: encoded_tags(tags),
                metadata: head.metadata,
                cache_control: head.cache_control,
                content_disposition: head.content_disposition,
                content_encoding: head.content_encoding,
                content_language: head.content_language,
                ..Default::default()
            },
        )
        .await?
        .version_id
    };
    if object.version_id.is_some()
        && version
            .as_deref()
            .is_none_or(|id| id.is_empty() || id == "null")
    {
        return Err(AppError::Storage(
            "The destination did not return a new object version.".into(),
        ));
    }
    Ok(Entry {
        object: ObjectId {
            bucket: bucket.into(),
            key: object.key.clone(),
            version_id: version,
        },
        marker: false,
    })
}

const COPY_PART_BYTES: u64 = 128 * 1024 * 1024;
const SINGLE_COPY_BYTES: u64 = 5_000_000_000;

async fn copy_in_storage(
    client: &Client,
    bucket: &str,
    object: &ObjectId,
    head: &HeadObjectOutput,
    etag: &str,
    tags: &[(String, String)],
) -> Result<Option<String>> {
    let size = head
        .content_length()
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| AppError::Storage("The source omitted a valid object size.".into()))?;
    let client = write_client(client);
    let source = objects::copy_source(object);
    if size <= SINGLE_COPY_BYTES {
        return Ok(client
            .copy_object()
            .bucket(bucket)
            .key(&object.key)
            .copy_source(source)
            .copy_source_if_match(etag)
            .set_if_none_match(object.version_id.is_none().then(|| "*".into()))
            .send()
            .await
            .map_err(|error| transfer_error(error, true, true))?
            .version_id);
    }
    let upload = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(&object.key)
        .set_content_type(head.content_type.clone())
        .set_metadata(head.metadata.clone())
        .set_cache_control(head.cache_control.clone())
        .set_content_disposition(head.content_disposition.clone())
        .set_content_encoding(head.content_encoding.clone())
        .set_content_language(head.content_language.clone())
        .set_tagging(encoded_tags(tags))
        .send()
        .await
        .map_err(|error| transfer_error(error, true, true))?;
    let upload_id = objects::required_id(upload.upload_id(), "copy upload ID")?;
    let result =
        finish_multipart_copy(&client, bucket, object, (&source, etag), &upload_id, size).await;
    if result.is_err()
        && let Err(error) = client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(&object.key)
            .upload_id(&upload_id)
            .send()
            .await
    {
        tracing::warn!(
            bucket,
            "Could not abort an incomplete migration upload: {}",
            transfer_error(error, true, true)
        );
    }
    result
}

async fn finish_multipart_copy(
    client: &Client,
    bucket: &str,
    object: &ObjectId,
    (source, etag): (&str, &str),
    upload_id: &str,
    size: u64,
) -> Result<Option<String>> {
    let part_bytes = COPY_PART_BYTES.max(size.div_ceil(10_000));
    let mut parts = Vec::new();
    for (index, offset) in (0..size).step_by(part_bytes as usize).enumerate() {
        let number = index as i32 + 1;
        let last = (offset + part_bytes).min(size) - 1;
        let part = client
            .upload_part_copy()
            .bucket(bucket)
            .key(&object.key)
            .upload_id(upload_id)
            .part_number(number)
            .copy_source(source)
            .copy_source_if_match(etag)
            .copy_source_range(format!("bytes={offset}-{last}"))
            .send()
            .await
            .map_err(|error| transfer_error(error, true, true))?;
        let etag = objects::required_id(
            part.copy_part_result().and_then(|value| value.e_tag()),
            "copied part ETag",
        )?;
        parts.push(
            CompletedPart::builder()
                .part_number(number)
                .e_tag(etag)
                .build(),
        );
    }
    let upload = CompletedMultipartUpload::builder()
        .set_parts(Some(parts))
        .build();
    Ok(client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(&object.key)
        .upload_id(upload_id)
        .set_if_none_match(object.version_id.is_none().then(|| "*".into()))
        .multipart_upload(upload)
        .send()
        .await
        .map_err(|error| transfer_error(error, true, true))?
        .version_id)
}

fn encoded_tags(tags: &[(String, String)]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.extend_pairs(tags.iter().map(|(key, value)| (key, value)));
    Some(form.finish().replace('+', "%20"))
}

async fn create_marker(client: &Client, bucket: &str, key: &str) -> Result<Entry> {
    let response = write_client(client)
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|error| transfer_error(error, true, true))?;
    if response.delete_marker != Some(true) {
        return Err(AppError::Storage(
            "The destination did not confirm creation of the delete marker.".into(),
        ));
    }
    let id = objects::required_id(response.version_id(), "delete marker version")?;
    Ok(Entry {
        object: ObjectId {
            bucket: bucket.into(),
            key: key.into(),
            version_id: Some(id),
        },
        marker: true,
    })
}

pub async fn remove_entry(client: &Client, entry: &Entry) -> Result<()> {
    let object = &entry.object;
    let etag = if object.version_id.is_none() {
        let head = objects::head(client, object).await?;
        Some(objects::required_id(head.e_tag(), "destination ETag")?)
    } else {
        None
    };
    write_client(client)
        .delete_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .set_if_match(etag)
        .send()
        .await
        .map_err(|error| transfer_error(error, true, true))?;
    Ok(())
}

pub async fn fingerprints(
    client: &Client,
    inventory: &Inventory,
    parallelism: usize,
) -> Result<Evidence> {
    let mut evidence = BTreeMap::new();
    let mut entries = inventory.iter();
    while let Some(first) = entries.next() {
        let mut tasks = JoinSet::new();
        for (key, entries) in std::iter::once(first).chain(entries.by_ref().take(parallelism - 1)) {
            let (client, key, entries) = (client.clone(), key.clone(), entries.clone());
            tasks.spawn(async move {
                let fingerprints = evidence_for_key(&client, &entries).await?;
                Ok::<_, AppError>((key, fingerprints))
            });
        }
        while let Some(result) = tasks.join_next().await {
            let (key, fingerprints) = result.map_err(|_| worker_failed())??;
            evidence.insert(key, fingerprints);
        }
    }
    Ok(evidence)
}

async fn evidence_for_key(client: &Client, entries: &[Entry]) -> Result<Vec<Fingerprint>> {
    let mut fingerprints = Vec::with_capacity(entries.len());
    for entry in entries {
        fingerprints.push(fingerprint(client, entry).await?);
    }
    Ok(fingerprints)
}

pub async fn fingerprint(client: &Client, entry: &Entry) -> Result<Fingerprint> {
    if entry.marker {
        return Ok(Fingerprint::Marker);
    }
    let object = &entry.object;
    let response = client
        .get_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .send()
        .await
        .map_err(|error| transfer_error(error, true, false))?;
    if response.ssekms_key_id().is_some()
        || response
            .server_side_encryption()
            .is_some_and(|algorithm| algorithm.as_str().starts_with("aws:kms"))
        || response.sse_customer_algorithm().is_some()
    {
        return Err(AppError::Invalid(
            "KMS and customer-encrypted objects are outside the migration perimeter.".into(),
        ));
    }
    let expected = response.content_length();
    let mut body = response.body;
    let mut digest = Sha256::new();
    let mut size = 0i64;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| {
            AppError::Storage("The object read ended before verification could finish.".into())
        })?;
        size = size.saturating_add(chunk.len() as i64);
        digest.update(&chunk);
    }
    if expected != Some(size) {
        return Err(AppError::Storage(
            "The object's byte count did not match its declared length.".into(),
        ));
    }
    let tags = exact_tags(client, object).await?;
    Ok(Fingerprint::Data {
        sha256: STANDARD.encode(digest.finalize()),
        tags,
    })
}

pub fn manifest_digest(evidence: &Evidence) -> Result<String> {
    Ok(STANDARD.encode(Sha256::digest(serde_json::to_vec(evidence)?)))
}

pub fn bind_versions(inventory: &Inventory, evidence: &Evidence) -> Result<VersionEvidence> {
    inventory
        .iter()
        .map(|(key, entries)| {
            let fingerprints = evidence
                .get(key)
                .filter(|values| values.len() == entries.len())
                .ok_or_else(divergence)?;
            let versions = entries
                .iter()
                .zip(fingerprints)
                .map(|(entry, value)| {
                    let version =
                        objects::required_id(entry.object.version_id.as_deref(), "source version")?;
                    Ok((version, value.clone()))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok((key.clone(), versions))
        })
        .collect()
}

pub fn remaining_verified(current: &VersionEvidence, reviewed: &VersionEvidence) -> bool {
    current.iter().all(|(key, versions)| {
        reviewed.get(key).is_some_and(|expected| {
            let remaining = versions
                .iter()
                .map(|(version, _)| version)
                .collect::<BTreeSet<_>>();
            expected
                .iter()
                .filter(|(version, _)| remaining.contains(version))
                .eq(versions)
        })
    })
}

fn divergence() -> AppError {
    AppError::Invalid("The destination diverged from the reviewed migration data.".into())
}

fn worker_failed() -> AppError {
    AppError::Storage("The migration worker could not finish the object check.".into())
}
