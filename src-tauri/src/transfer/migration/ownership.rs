use aws_sdk_s3::{
    error::ProvideErrorMetadata,
    types::{Tag, Tagging},
};
use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::service::Session,
    transfer::security::write_client,
};

use super::{
    execution_model::Ownership,
    history,
    model::Migration,
    preflight::{create_new_bucket, require_absent},
    repository::MigrationRepository,
};

pub const RECEIPT_TAG: &str = "storeforge-migration-receipt";

pub async fn prepare(
    repository: &MigrationRepository,
    record: &mut Migration,
    target: &Session,
    index: usize,
) -> Result<()> {
    let bucket = record.run.as_ref().unwrap().buckets[index]
        .mapping
        .target
        .clone();
    let ownership = record.run.as_ref().unwrap().buckets[index].ownership;
    let token = match ownership {
        Ownership::Owned { token } => return verify(target, &bucket, token).await,
        Ownership::Created { token } => token,
        Ownership::Creating { token } => {
            if verify(target, &bucket, token).await.is_ok() {
                checkpoint(repository, record, index, Ownership::Owned { token })?;
                return Ok(());
            }
            require_absent(&target.s3, &bucket).await.map_err(|_| AppError::Invalid(
                "Destination creation was interrupted without an ownership receipt. Inspect the bucket before resuming.".into(),
            ))?;
            create(repository, record, target, index, &bucket, token).await?;
            token
        }
        Ownership::Pending => {
            require_absent(&target.s3, &bucket).await?;
            let token = Uuid::new_v4();
            checkpoint(repository, record, index, Ownership::Creating { token })?;
            create(repository, record, target, index, &bucket, token).await?;
            token
        }
    };
    let mut tags = tags(target, &bucket).await?;
    let expected = token.to_string();
    if let Some(existing) = tags.iter().find(|tag| tag.key() == RECEIPT_TAG) {
        if existing.value() != expected {
            return Err(unowned());
        }
    } else {
        if !tags.is_empty() || !history::inventory(target, &bucket, true).await?.is_empty() {
            return Err(unowned());
        }
        tags.push(receipt(token)?);
        write_tags(target, &bucket, tags).await?;
    }
    verify(target, &bucket, token).await?;
    checkpoint(repository, record, index, Ownership::Owned { token })
}

async fn create(
    repository: &MigrationRepository,
    record: &mut Migration,
    target: &Session,
    index: usize,
    bucket: &str,
    token: Uuid,
) -> Result<()> {
    if let Err(error) = create_new_bucket(target, bucket).await {
        if !matches!(error, AppError::AmbiguousWrite(_)) {
            checkpoint(repository, record, index, Ownership::Pending)?;
        }
        return Err(error);
    }
    checkpoint(repository, record, index, Ownership::Created { token })
}

fn checkpoint(
    repository: &MigrationRepository,
    record: &mut Migration,
    index: usize,
    ownership: Ownership,
) -> Result<()> {
    record.run.as_mut().unwrap().buckets[index].ownership = ownership;
    repository.save(record)
}

pub async fn probe_tagging(target: &Session, bucket: &str) -> Result<()> {
    let token = Uuid::new_v4();
    write_tags(target, bucket, vec![receipt(token)?]).await?;
    verify(target, bucket, token).await
}

pub async fn verify(target: &Session, bucket: &str, token: Uuid) -> Result<()> {
    let expected = token.to_string();
    if tags(target, bucket)
        .await?
        .iter()
        .filter(|tag| tag.key() == RECEIPT_TAG)
        .map(Tag::value)
        .eq([expected.as_str()])
    {
        return Ok(());
    }
    Err(unowned())
}

pub async fn tags(session: &Session, bucket: &str) -> Result<Vec<Tag>> {
    match session.s3.get_bucket_tagging().bucket(bucket).send().await {
        Ok(output) => Ok(output.tag_set),
        Err(error)
            if error
                .as_service_error()
                .and_then(ProvideErrorMetadata::code)
                == Some("NoSuchTagSet") =>
        {
            Ok(Vec::new())
        }
        Err(_) => Err(AppError::Storage(
            "The provider could not verify the bucket's tags and migration ownership.".into(),
        )),
    }
}

pub async fn copy_tags(
    source: &Session,
    target: &Session,
    from: &str,
    to: &str,
    token: Uuid,
) -> Result<()> {
    verify(target, to, token).await?;
    let mut expected = tags(source, from).await?;
    if expected.len() >= 50 || expected.iter().any(|tag| tag.key() == RECEIPT_TAG) {
        return Err(AppError::Invalid(
            "Source bucket tags leave no slot for the migration ownership receipt.".into(),
        ));
    }
    expected.push(receipt(token)?);
    write_tags(target, to, expected.clone()).await?;
    if sorted_tags(tags(target, to).await?) != sorted_tags(expected) {
        return Err(AppError::Storage(
            "The provider did not preserve the copied bucket tags.".into(),
        ));
    }
    Ok(())
}

fn sorted_tags(tags: Vec<Tag>) -> Vec<(String, String)> {
    let mut tags: Vec<_> = tags.into_iter().map(|tag| (tag.key, tag.value)).collect();
    tags.sort_unstable();
    tags
}

async fn write_tags(target: &Session, bucket: &str, tags: Vec<Tag>) -> Result<()> {
    let tagging = Tagging::builder()
        .set_tag_set(Some(tags))
        .build()
        .map_err(|_| AppError::Invalid("Migration bucket tags are invalid.".into()))?;
    write_client(&target.s3)
        .put_bucket_tagging()
        .bucket(bucket)
        .tagging(tagging)
        .send()
        .await
        .map_err(|_| {
            AppError::Storage(
                "The provider could not write the migration ownership receipt.".into(),
            )
        })?;
    Ok(())
}

fn receipt(token: Uuid) -> Result<Tag> {
    Tag::builder()
        .key(RECEIPT_TAG)
        .value(token.to_string())
        .build()
        .map_err(|_| unowned())
}

fn unowned() -> AppError {
    AppError::Invalid("The destination's migration ownership receipt could not be verified.".into())
}
