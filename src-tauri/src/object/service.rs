use std::collections::HashSet;
use std::time::Duration;

use aws_sdk_s3::{
    Client,
    presigning::PresigningConfig,
    primitives::{ByteStream, DateTime, DateTimeFormat},
    types::{
        GlacierJobParameters, MetadataDirective, ObjectCannedAcl, ObjectLockLegalHold,
        ObjectLockLegalHoldStatus, ObjectLockRetention, ObjectLockRetentionMode, RestoreRequest,
        Tag as S3Tag, Tagging, Tier,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{
    common::error::{AppError, Result, storage_error},
    transfer::security::{ReadAccess, customer_headers, transfer_error},
};

use super::{
    model::{
        MetadataUpdate, MultipartUpload, ObjectChange, ObjectDetails, ObjectId, Preview,
        PreviewKind, Tag,
    },
    repository,
};

const OBJECT_PREVIEW_LIMIT: i64 = 50 * 1024 * 1024;
const TEXT_PREVIEW_LIMIT: i64 = 64 * 1024;

pub struct ObjectService;

impl ObjectService {
    pub(crate) async fn details(
        client: &Client,
        object: &ObjectId,
        access: ReadAccess<'_>,
    ) -> Result<ObjectDetails> {
        let head = repository::head_with_access(client, object, access).await?;
        Ok(ObjectDetails {
            content_type: head.content_type,
            size: head.content_length,
            etag: head.e_tag,
            modified: head.last_modified.map(|value| value.to_string()),
            version_id: head.version_id,
            storage_class: head.storage_class.map(|value| value.to_string()),
            retain_until: head
                .object_lock_retain_until_date
                .map(|value| value.to_string()),
            retention_mode: head.object_lock_mode.map(|value| value.to_string()),
            legal_hold: head
                .object_lock_legal_hold_status
                .map(|value| value.to_string()),
            encryption: head
                .sse_customer_algorithm
                .map(|_| "SSE-C".into())
                .or_else(|| access.customer.is_some().then(|| "SSE-C".into()))
                .or_else(|| head.server_side_encryption.map(|value| value.to_string())),
            restore: head.restore,
            cache_control: head.cache_control,
            content_disposition: head.content_disposition,
            content_encoding: head.content_encoding,
            content_language: head.content_language,
            expires: head.expires_string,
            metadata: head.metadata.unwrap_or_default(),
        })
    }

    pub async fn tags(client: &Client, object: &ObjectId) -> Result<Vec<Tag>> {
        let output = client
            .get_object_tagging()
            .bucket(&object.bucket)
            .key(&object.key)
            .set_version_id(object.version_id.clone())
            .send()
            .await
            .map_err(storage_error)?;
        Ok(output
            .tag_set
            .into_iter()
            .map(|tag| Tag {
                key: tag.key,
                value: tag.value,
            })
            .collect())
    }

    pub(crate) async fn preview(
        client: &Client,
        object: &ObjectId,
        access: ReadAccess<'_>,
    ) -> Result<Preview> {
        let head = repository::head_with_access(client, object, access).await?;
        let size = head
            .content_length()
            .filter(|size| *size >= 0)
            .ok_or_else(|| {
                AppError::Storage("The endpoint did not report the object's size.".into())
            })?;
        let content_type = preview_content_type(head.content_type(), &object.key);
        let kind = preview_kind(&content_type);
        let oversized = size > OBJECT_PREVIEW_LIMIT;
        let truncated = !oversized && kind == PreviewKind::Text && size > TEXT_PREVIEW_LIMIT;
        let data = if oversized || size == 0 {
            String::new()
        } else {
            let length = if truncated { TEXT_PREVIEW_LIMIT } else { size };
            let range = truncated.then(|| format!("bytes=0-{}", length - 1));
            let bytes =
                read_preview(client, object, head.e_tag, range, length as usize, access).await?;
            STANDARD.encode(bytes)
        };
        Ok(Preview {
            content_type,
            kind,
            data,
            truncated,
            oversized,
        })
    }

    pub async fn share_url(client: &Client, object: &ObjectId, minutes: u32) -> Result<String> {
        if !(1..=10_080).contains(&minutes) {
            return Err(AppError::Invalid(
                "Expiry must be between 1 minute and 7 days.".into(),
            ));
        }
        let head = match repository::head(client, object).await {
            Ok(head) if head.sse_customer_algorithm.is_none() => head,
            Err(error @ AppError::Missing(_)) => return Err(error),
            _ => return Err(AppError::Invalid(
                "The endpoint could not confirm this object can be shared without a customer key."
                    .into(),
            )),
        };
        let config = PresigningConfig::expires_in(Duration::from_secs(u64::from(minutes) * 60))
            .map_err(|error| AppError::Invalid(error.to_string()))?;
        let request = client
            .get_object()
            .bucket(&object.bucket)
            .key(&object.key)
            .set_version_id(object.version_id.clone().or(head.version_id))
            .presigned(config)
            .await
            .map_err(storage_error)?;
        Ok(request.uri().to_string())
    }

    pub async fn create_folder(client: &Client, bucket: &str, key: &str) -> Result<()> {
        if !key.ends_with('/') {
            return Err(AppError::Invalid("A folder key must end with /.".into()));
        }
        repository::ensure_absent(client, bucket, key).await?;
        client
            .put_object()
            .bucket(bucket)
            .key(key)
            .if_none_match("*")
            .body(ByteStream::from_static(b""))
            .send()
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    pub async fn change(client: &Client, object: &ObjectId, change: ObjectChange) -> Result<()> {
        Self::change_with_version(client, object, change)
            .await
            .map(|_| ())
    }

    pub async fn change_with_version(
        client: &Client,
        object: &ObjectId,
        change: ObjectChange,
    ) -> Result<Option<String>> {
        match change {
            ObjectChange::RestoreVersion => {
                return restore_version(client, object).await;
            }
            ObjectChange::Delete => Self::delete(client, object).await,
            ObjectChange::SetTags { tags } => set_tags(client, object, tags).await,
            ObjectChange::SetMetadata { update } => {
                Self::update_copy_attributes(client, object, Some(update), None).await
            }
            ObjectChange::SetAcl { acl } => set_acl(client, object, acl).await,
            ObjectChange::LegalHold { enabled } => set_legal_hold(client, object, enabled).await,
            ObjectChange::Retention { mode, until } => {
                Self::set_retention(client, object, mode, &until, false).await
            }
            ObjectChange::RestoreArchive { days, tier } => {
                restore_archive(client, object, days, tier).await
            }
        }?;
        Ok(None)
    }

    pub async fn delete(client: &Client, object: &ObjectId) -> Result<()> {
        client
            .delete_object()
            .bucket(&object.bucket)
            .key(&object.key)
            .set_version_id(object.version_id.clone())
            .send()
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    pub async fn multipart(
        client: &Client,
        bucket: &str,
        prefix: &str,
    ) -> Result<Vec<MultipartUpload>> {
        let mut result = Vec::new();
        let mut key_marker = None;
        let mut upload_marker = None;
        loop {
            let page = client
                .list_multipart_uploads()
                .bucket(bucket)
                .prefix(prefix)
                .set_key_marker(key_marker.clone())
                .set_upload_id_marker(upload_marker.clone())
                .send()
                .await
                .map_err(storage_error)?;
            result.extend(page.uploads().iter().filter_map(|upload| {
                Some(MultipartUpload {
                    key: upload.key()?.into(),
                    upload_id: upload.upload_id()?.into(),
                    initiated: upload.initiated().map(ToString::to_string),
                })
            }));
            if !page.is_truncated().unwrap_or(false) {
                break;
            }
            let next_key = page.next_key_marker().map(str::to_owned);
            let next_upload = page.next_upload_id_marker().map(str::to_owned);
            if next_key == key_marker && next_upload == upload_marker {
                return Err(AppError::Storage(
                    "The endpoint repeated a multipart cursor.".into(),
                ));
            }
            key_marker = next_key;
            upload_marker = next_upload;
        }
        Ok(result)
    }

    pub async fn abort_multipart(
        client: &Client,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<()> {
        client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    pub async fn acl(client: &Client, object: &ObjectId) -> Result<Vec<String>> {
        let output = client
            .get_object_acl()
            .bucket(&object.bucket)
            .key(&object.key)
            .set_version_id(object.version_id.clone())
            .send()
            .await
            .map_err(storage_error)?;
        Ok(output
            .grants()
            .iter()
            .filter_map(|grant| {
                let grantee = grant.grantee()?;
                let identity = grantee
                    .id()
                    .or(grantee.uri())
                    .or(grantee.email_address())
                    .unwrap_or("Unknown");
                let permission = grant.permission()?;
                Some(format!("{permission} · {identity}"))
            })
            .collect())
    }
}

fn preview_content_type(content_type: Option<&str>, key: &str) -> String {
    let declared = content_type
        .and_then(|value| value.split(';').next())
        .map(|mime| mime.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(
        declared.as_str(),
        "" | "application/octet-stream" | "binary/octet-stream"
    ) {
        return mime_guess::from_path(key.trim_end())
            .first_or_octet_stream()
            .essence_str()
            .to_owned();
    }
    declared
}

fn preview_kind(content_type: &str) -> PreviewKind {
    match content_type {
        "image/png" | "image/jpeg" | "image/webp" | "image/gif" | "image/avif" | "image/bmp" => {
            PreviewKind::Image
        }
        "audio/mpeg" | "audio/mp4" | "audio/aac" | "audio/wav" | "audio/x-wav" | "audio/wave"
        | "audio/ogg" | "audio/webm" | "audio/flac" => PreviewKind::Audio,
        "video/mp4" | "video/webm" | "video/ogg" | "video/quicktime" => PreviewKind::Video,
        "application/pdf" => PreviewKind::Pdf,
        _ => PreviewKind::Text,
    }
}

async fn read_preview(
    client: &Client,
    object: &ObjectId,
    etag: Option<String>,
    range: Option<String>,
    length: usize,
    access: ReadAccess<'_>,
) -> Result<Vec<u8>> {
    let ranged = range.is_some();
    let get = client
        .get_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .set_if_match(etag)
        .set_range(range);
    let mut output = customer_headers!(get, access.customer)
        .send()
        .await
        .map_err(|error| transfer_error(error, access.sensitive, false))?;
    let mut bytes = Vec::with_capacity(length);
    while let Some(chunk) = output
        .body
        .try_next()
        .await
        .map_err(|error| AppError::Storage(error.to_string()))?
    {
        let remaining = length - bytes.len();
        if !ranged && chunk.len() > remaining {
            return Err(AppError::Storage(
                "The object size changed during preview.".into(),
            ));
        }
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if ranged && bytes.len() == length {
            break;
        }
    }
    if bytes.len() != length {
        return Err(AppError::Storage(
            "The object size changed during preview.".into(),
        ));
    }
    Ok(bytes)
}

async fn restore_version(client: &Client, object: &ObjectId) -> Result<Option<String>> {
    let output = client
        .copy_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .copy_source(repository::copy_source(object))
        .send()
        .await
        .map_err(storage_error)?;
    Ok(output.version_id)
}

fn validate_tags(tags: &[Tag]) -> Result<()> {
    let mut keys = HashSet::new();
    if tags.len() > 10
        || tags
            .iter()
            .any(|tag| tag.key.is_empty() || !keys.insert(&tag.key))
    {
        return Err(AppError::Invalid(
            "Use up to 10 tags with distinct, nonempty keys.".into(),
        ));
    }
    Ok(())
}

async fn set_tags(client: &Client, object: &ObjectId, tags: Vec<Tag>) -> Result<()> {
    validate_tags(&tags)?;
    if tags.is_empty() {
        client
            .delete_object_tagging()
            .bucket(&object.bucket)
            .key(&object.key)
            .set_version_id(object.version_id.clone())
            .send()
            .await
            .map_err(storage_error)?;
        return Ok(());
    }
    let tags = tags
        .into_iter()
        .map(|tag| S3Tag::builder().key(tag.key).value(tag.value).build())
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    let tagging = Tagging::builder()
        .set_tag_set(Some(tags))
        .build()
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    client
        .put_object_tagging()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .tagging(tagging)
        .send()
        .await
        .map_err(storage_error)?;
    Ok(())
}

impl ObjectService {
    pub async fn update_copy_attributes(
        client: &Client,
        object: &ObjectId,
        update: Option<MetadataUpdate>,
        storage_class: Option<&str>,
    ) -> Result<()> {
        if object.version_id.is_some() {
            return Err(AppError::Invalid(
                "Only the current version supports metadata or storage class replacement.".into(),
            ));
        }
        let current = repository::head(client, object).await?;
        let mut copy = client
            .copy_object()
            .bucket(&object.bucket)
            .key(&object.key)
            .copy_source(repository::copy_source(object))
            .set_copy_source_if_match(current.e_tag)
            .set_website_redirect_location(current.website_redirect_location)
            .set_storage_class(
                storage_class
                    .map(aws_sdk_s3::types::StorageClass::from)
                    .or(current.storage_class),
            )
            .set_server_side_encryption(current.server_side_encryption)
            .set_ssekms_key_id(current.ssekms_key_id);
        if let Some(update) = update {
            let expires = update
                .expires
                .as_deref()
                .map(parse_content_expiry)
                .transpose()?;
            copy = copy
                .metadata_directive(MetadataDirective::Replace)
                .set_metadata(Some(update.metadata))
                .set_content_type(update.content_type)
                .set_cache_control(update.cache_control)
                .set_content_disposition(update.content_disposition)
                .set_content_encoding(update.content_encoding)
                .set_content_language(update.content_language)
                .set_expires(expires);
        }
        copy.send().await.map_err(storage_error)?;
        Ok(())
    }
}

fn parse_content_expiry(value: &str) -> Result<DateTime> {
    DateTime::from_str(value, DateTimeFormat::HttpDate)
        .or_else(|_| DateTime::from_str(value, DateTimeFormat::DateTime))
        .map_err(|_| {
            AppError::Invalid("Use an HTTP date or an ISO 8601 timestamp for Expires.".into())
        })
}

async fn set_acl(client: &Client, object: &ObjectId, acl: String) -> Result<()> {
    client
        .put_object_acl()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .acl(ObjectCannedAcl::from(acl.as_str()))
        .send()
        .await
        .map_err(storage_error)?;
    Ok(())
}

async fn set_legal_hold(client: &Client, object: &ObjectId, enabled: bool) -> Result<()> {
    let status = if enabled {
        ObjectLockLegalHoldStatus::On
    } else {
        ObjectLockLegalHoldStatus::Off
    };
    client
        .put_object_legal_hold()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .legal_hold(ObjectLockLegalHold::builder().status(status).build())
        .send()
        .await
        .map_err(storage_error)?;
    Ok(())
}

impl ObjectService {
    pub async fn set_retention(
        client: &Client,
        object: &ObjectId,
        mode: super::model::RetentionMode,
        until: &str,
        bypass_governance: bool,
    ) -> Result<()> {
        use super::model::RetentionMode;
        let mode = match mode {
            RetentionMode::Governance => ObjectLockRetentionMode::Governance,
            RetentionMode::Compliance => ObjectLockRetentionMode::Compliance,
        };
        let until = DateTime::from_str(until, DateTimeFormat::DateTime).map_err(|_| {
            AppError::Invalid(
                "Use an ISO 8601 retention timestamp, including the time zone.".into(),
            )
        })?;
        client
            .put_object_retention()
            .bucket(&object.bucket)
            .key(&object.key)
            .set_version_id(object.version_id.clone())
            .set_bypass_governance_retention(bypass_governance.then_some(true))
            .retention(
                ObjectLockRetention::builder()
                    .mode(mode)
                    .retain_until_date(until)
                    .build(),
            )
            .send()
            .await
            .map_err(storage_error)?;
        Ok(())
    }
}

async fn restore_archive(
    client: &Client,
    object: &ObjectId,
    days: i32,
    tier: String,
) -> Result<()> {
    if days < 1 {
        return Err(AppError::Invalid(
            "Enter at least one day for archive access.".into(),
        ));
    }
    client
        .restore_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .restore_request(
            RestoreRequest::builder()
                .days(days)
                .glacier_job_parameters(
                    GlacierJobParameters::builder()
                        .tier(Tier::from(tier.as_str()))
                        .build()
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                )
                .build(),
        )
        .send()
        .await
        .map_err(storage_error)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/object/service.rs"]
pub(crate) mod tests;
