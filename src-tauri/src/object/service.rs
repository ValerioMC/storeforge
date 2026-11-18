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

use crate::common::error::{AppError, Result, storage_error};

use super::{
    model::{MetadataUpdate, MultipartUpload, ObjectChange, ObjectDetails, ObjectId, Preview, Tag},
    repository,
};

const PREVIEW_LIMIT: i64 = 2 * 1024 * 1024;

pub struct ObjectService;

impl ObjectService {
    pub async fn details(client: &Client, object: &ObjectId) -> Result<ObjectDetails> {
        let head = repository::head(client, object).await?;
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
            encryption: head.server_side_encryption.map(|value| value.to_string()),
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

    pub async fn preview(client: &Client, object: &ObjectId) -> Result<Preview> {
        let head = repository::head(client, object).await?;
        let size = head.content_length().unwrap_or_default();
        let content_type = head
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_owned();
        if size == 0 {
            return Ok(Preview {
                content_type,
                data: String::new(),
                truncated: false,
            });
        }
        let mut output = client
            .get_object()
            .bucket(&object.bucket)
            .key(&object.key)
            .set_version_id(object.version_id.clone())
            .set_if_match(head.e_tag)
            .range(format!("bytes=0-{}", PREVIEW_LIMIT - 1))
            .send()
            .await
            .map_err(storage_error)?;
        let mut bytes = Vec::new();
        while let Some(chunk) = output
            .body
            .try_next()
            .await
            .map_err(|error| AppError::Storage(error.to_string()))?
        {
            let remaining = PREVIEW_LIMIT as usize - bytes.len();
            bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            if bytes.len() == PREVIEW_LIMIT as usize {
                break;
            }
        }
        Ok(Preview {
            content_type,
            data: STANDARD.encode(bytes),
            truncated: size > PREVIEW_LIMIT,
        })
    }

    pub async fn share_url(client: &Client, object: &ObjectId, minutes: u32) -> Result<String> {
        if !(1..=10_080).contains(&minutes) {
            return Err(AppError::Invalid(
                "Expiry must be between 1 minute and 7 days.".into(),
            ));
        }
        let config = PresigningConfig::expires_in(Duration::from_secs(u64::from(minutes) * 60))
            .map_err(|error| AppError::Invalid(error.to_string()))?;
        let request = client
            .get_object()
            .bucket(&object.bucket)
            .key(&object.key)
            .set_version_id(object.version_id.clone())
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
        match change {
            ObjectChange::Delete => Self::delete(client, object).await,
            ObjectChange::RestoreVersion => {
                Self::copy(client, object, &object.bucket, &object.key).await
            }
            ObjectChange::SetTags { tags } => set_tags(client, object, tags).await,
            ObjectChange::SetMetadata { update } => set_metadata(client, object, update).await,
            ObjectChange::SetAcl { acl } => set_acl(client, object, acl).await,
            ObjectChange::LegalHold { enabled } => set_legal_hold(client, object, enabled).await,
            ObjectChange::Retention { mode, until } => {
                set_retention(client, object, mode, &until).await
            }
            ObjectChange::RestoreArchive { days, tier } => {
                restore_archive(client, object, days, tier).await
            }
        }
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

    pub async fn copy(client: &Client, source: &ObjectId, bucket: &str, key: &str) -> Result<()> {
        client
            .copy_object()
            .bucket(bucket)
            .key(key)
            .copy_source(repository::copy_source(source))
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

async fn set_metadata(client: &Client, object: &ObjectId, update: MetadataUpdate) -> Result<()> {
    if object.version_id.is_some() {
        return Err(AppError::Invalid(
            "Only the current version supports metadata replacement.".into(),
        ));
    }
    let current = repository::head(client, object).await?;
    client
        .copy_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .copy_source(repository::copy_source(object))
        .set_copy_source_if_match(current.e_tag)
        .metadata_directive(MetadataDirective::Replace)
        .set_metadata(Some(update.metadata))
        .set_content_type(update.content_type)
        .set_cache_control(update.cache_control)
        .set_content_disposition(current.content_disposition)
        .set_content_encoding(current.content_encoding)
        .set_content_language(current.content_language)
        .set_website_redirect_location(current.website_redirect_location)
        .set_storage_class(current.storage_class)
        .set_server_side_encryption(current.server_side_encryption)
        .set_ssekms_key_id(current.ssekms_key_id)
        .send()
        .await
        .map_err(storage_error)?;
    Ok(())
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

async fn set_retention(
    client: &Client,
    object: &ObjectId,
    mode: super::model::RetentionMode,
    until: &str,
) -> Result<()> {
    use super::model::RetentionMode;
    let mode = match mode {
        RetentionMode::Governance => ObjectLockRetentionMode::Governance,
        RetentionMode::Compliance => ObjectLockRetentionMode::Compliance,
    };
    let until = DateTime::from_str(until, DateTimeFormat::DateTime).map_err(|_| {
        AppError::Invalid("Use an ISO 8601 retention timestamp, including the time zone.".into())
    })?;
    client
        .put_object_retention()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
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
mod tests;
