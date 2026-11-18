use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use aws_sdk_s3::{
    Client,
    primitives::ByteStream,
    types::{CompletedMultipartUpload, CompletedPart},
};
use tempfile::NamedTempFile;
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::{
    common::error::{AppError, Result, storage_error},
    object::{model::ObjectId, repository},
};

const MULTIPART_THRESHOLD: u64 = 64 * 1024 * 1024;
const PART_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PARTS: u64 = 10_000;

#[derive(Default)]
pub struct UploadOptions {
    pub content_type: String,
    pub overwrite: bool,
    pub metadata: Option<HashMap<String, String>>,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub content_encoding: Option<String>,
    pub content_language: Option<String>,
}

pub struct UploadReceipt {
    pub etag: Option<String>,
    pub version_id: Option<String>,
}

pub async fn upload(
    client: &Client,
    bucket: &str,
    key: &str,
    path: &Path,
    options: UploadOptions,
) -> Result<UploadReceipt> {
    let info = tokio::fs::metadata(path).await?;
    if !info.is_file() {
        return Err(AppError::Invalid("Select regular files to upload.".into()));
    }
    if !options.overwrite {
        repository::ensure_absent(client, bucket, key).await?;
    }
    if info.len() >= MULTIPART_THRESHOLD {
        return multipart_upload(client, bucket, key, path, info.len(), options).await;
    }
    let body = ByteStream::from_path(path)
        .await
        .map_err(|error| AppError::File(error.to_string()))?;
    let uploaded = client
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_type(options.content_type)
        .set_metadata(options.metadata)
        .set_cache_control(options.cache_control)
        .set_content_disposition(options.content_disposition)
        .set_content_encoding(options.content_encoding)
        .set_content_language(options.content_language)
        .set_if_none_match((!options.overwrite).then(|| "*".into()))
        .body(body)
        .send()
        .await
        .map_err(storage_error)?;
    Ok(UploadReceipt {
        etag: uploaded.e_tag,
        version_id: uploaded.version_id,
    })
}

async fn multipart_upload(
    client: &Client,
    bucket: &str,
    key: &str,
    path: &Path,
    size: u64,
    options: UploadOptions,
) -> Result<UploadReceipt> {
    let started = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .content_type(options.content_type)
        .set_metadata(options.metadata)
        .set_cache_control(options.cache_control)
        .set_content_disposition(options.content_disposition)
        .set_content_encoding(options.content_encoding)
        .set_content_language(options.content_language)
        .send()
        .await
        .map_err(storage_error)?;
    let upload_id = repository::required_id(started.upload_id(), "multipart upload")?;
    let result = upload_parts(
        client,
        bucket,
        key,
        path,
        size,
        &upload_id,
        options.overwrite,
    )
    .await;
    if result.is_err()
        && let Err(error) = client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
    {
        tracing::warn!(%error, "Could not abort a failed multipart upload");
    }
    result
}

async fn upload_parts(
    client: &Client,
    bucket: &str,
    key: &str,
    path: &Path,
    size: u64,
    upload_id: &str,
    overwrite: bool,
) -> Result<UploadReceipt> {
    let part_bytes = PART_BYTES.max(size.div_ceil(MAX_PARTS));
    let mut parts = Vec::new();
    for (index, offset) in (0..size).step_by(part_bytes as usize).enumerate() {
        let body = ByteStream::read_from()
            .path(path)
            .offset(offset)
            .length(aws_sdk_s3::primitives::Length::Exact(
                part_bytes.min(size - offset),
            ))
            .build()
            .await
            .map_err(|error| AppError::File(error.to_string()))?;
        let part_number = index as i32 + 1;
        let response = client
            .upload_part()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number)
            .body(body)
            .send()
            .await
            .map_err(storage_error)?;
        let etag = repository::required_id(response.e_tag(), "part ETag")?;
        parts.push(
            CompletedPart::builder()
                .part_number(part_number)
                .e_tag(etag)
                .build(),
        );
    }
    if !overwrite {
        repository::ensure_absent(client, bucket, key).await?;
    }
    let uploaded = client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .set_if_none_match((!overwrite).then(|| "*".into()))
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .set_parts(Some(parts))
                .build(),
        )
        .send()
        .await
        .map_err(storage_error)?;
    Ok(UploadReceipt {
        etag: uploaded.e_tag,
        version_id: uploaded.version_id,
    })
}

pub async fn download(
    client: &Client,
    object: &ObjectId,
    path: &Path,
    replace: bool,
) -> Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| AppError::Invalid("Choose a local destination.".into()))?;
    let mut temporary = NamedTempFile::new_in(directory)?;
    stream_to_file(client, object, &mut temporary, None).await?;
    if replace {
        temporary.persist(path).map_err(|error| error.error)?;
    } else {
        temporary
            .persist_noclobber(path)
            .map_err(|error| error.error)?;
    }
    Ok(())
}

pub async fn stream_to_file(
    client: &Client,
    object: &ObjectId,
    file: &mut NamedTempFile,
    etag: Option<String>,
) -> Result<u64> {
    let output = client
        .get_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .set_if_match(etag)
        .send()
        .await
        .map_err(storage_error)?;
    let mut writer = tokio::fs::File::from_std(file.reopen()?);
    let size = tokio::io::copy(&mut output.body.into_async_read(), &mut writer).await?;
    writer.sync_all().await?;
    Ok(size)
}

pub fn file_name(key: &str) -> Result<&str> {
    let name = key.rsplit('/').next().unwrap_or_default();
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.contains(['\\', ':', '*', '?', '"', '<', '>', '|'])
    {
        return Err(AppError::Invalid(
            "This object key is unsafe as a local filename. Download it with a chosen filename."
                .into(),
        ));
    }
    Ok(name)
}

pub fn archive_safe_name(key: &str) -> Result<String> {
    if key.split('/').any(|segment| {
        segment.is_empty() || matches!(segment, "." | "..") || segment.contains(['\\', ':', '\0'])
    }) {
        return Err(AppError::Invalid(format!(
            "Cannot safely archive object key {key:?}."
        )));
    }
    Ok(key.into())
}

pub fn archive_writer(target: &Path) -> Result<(NamedTempFile, ZipWriter<std::fs::File>)> {
    let directory = target
        .parent()
        .ok_or_else(|| AppError::Invalid("Choose an archive destination.".into()))?;
    let file = NamedTempFile::new_in(directory)?;
    let writer = ZipWriter::new(file.reopen()?);
    Ok((file, writer))
}

pub fn archive_add(
    writer: &mut ZipWriter<std::fs::File>,
    key: &str,
    source: &mut impl Read,
) -> Result<()> {
    writer
        .start_file(
            archive_safe_name(key)?,
            SimpleFileOptions::default().large_file(true),
        )
        .map_err(|error| AppError::File(error.to_string()))?;
    std::io::copy(source, writer)?;
    writer.flush()?;
    Ok(())
}

pub fn path_name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Invalid("Choose a file with a UTF-8 name.".into()))
}

pub fn download_target(target: &Path, directory: bool, object: &ObjectId) -> Result<PathBuf> {
    if directory {
        return Ok(target.join(file_name(&object.key)?));
    }
    Ok(target.to_owned())
}
