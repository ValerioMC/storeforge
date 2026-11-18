use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use aws_sdk_s3::Client;
use chrono::Utc;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{
    common::error::{AppError, Result, storage_error},
    connection::service::ConnectionService,
    object::{
        model::{Location, ObjectChange, ObjectId, Tag},
        repository as objects,
        service::ObjectService,
    },
};

use super::{
    files::{self, UploadOptions, UploadReceipt},
    inventory,
    model::{Job, JobFailure, JobStatus, Transfer},
    repository::JobRepository,
};

#[derive(Clone)]
pub struct TransferService {
    jobs: Arc<JobRepository>,
    connections: Arc<ConnectionService>,
}

struct CopyPlan {
    source: Location,
    target: Location,
    objects: Vec<ObjectId>,
    move_source: bool,
    overwrite: bool,
}

struct Progress<'a> {
    id: Uuid,
    jobs: &'a JobRepository,
}

impl TransferService {
    pub fn new(jobs: JobRepository, connections: Arc<ConnectionService>) -> Self {
        Self {
            jobs: Arc::new(jobs),
            connections,
        }
    }

    pub fn list(&self) -> Result<Vec<Job>> {
        self.jobs.list()
    }

    pub fn cancel(&self, id: Uuid) -> Result<()> {
        self.jobs.update(id, |job| {
            job.cancel_requested = job.status == JobStatus::Running
        })?;
        Ok(())
    }

    pub fn start(&self, transfer: Transfer) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let label = transfer.label().into();
        self.jobs.insert(Job {
            id,
            label,
            status: JobStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            total: 0,
            succeeded: 0,
            failed: 0,
            failures: Vec::new(),
            current: None,
            cancel_requested: false,
        })?;
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            let progress = Progress {
                id,
                jobs: &service.jobs,
            };
            let result = service.run(&progress, transfer).await;
            if let Err(error) = progress.finish(result) {
                tracing::error!(%error, %id, "Could not persist transfer finish");
            }
        });
        Ok(id)
    }

    async fn run(&self, progress: &Progress<'_>, transfer: Transfer) -> Result<()> {
        match transfer {
            Transfer::Upload {
                target,
                paths,
                overwrite,
            } => self.upload(progress, target, paths, overwrite).await,
            Transfer::Download {
                connection,
                objects,
                target,
                directory,
            } => {
                self.download(progress, connection, objects, &target, directory)
                    .await
            }
            Transfer::Archive {
                connection,
                objects,
                target,
            } => self.archive(progress, connection, objects, &target).await,
            Transfer::Delete {
                connection,
                objects,
            } => {
                self.change_objects(progress, connection, objects, None)
                    .await
            }
            Transfer::Tag {
                connection,
                objects,
                tags,
            } => {
                self.change_objects(progress, connection, objects, Some(tags))
                    .await
            }
            Transfer::Copy {
                source,
                objects,
                target,
                move_source,
                overwrite,
            } => {
                self.copy(
                    progress,
                    CopyPlan {
                        source,
                        target,
                        objects,
                        move_source,
                        overwrite,
                    },
                )
                .await
            }
            Transfer::Migrate {
                source,
                target,
                overwrite,
            } => {
                let client = self.connections.session(source.connection)?.s3;
                let rows = inventory::objects(&client, &source).await?;
                let objects = rows
                    .into_iter()
                    .map(|row| object_id(&source, row.key))
                    .collect();
                self.copy(
                    progress,
                    CopyPlan {
                        source,
                        target,
                        objects,
                        move_source: false,
                        overwrite,
                    },
                )
                .await
            }
            Transfer::Purge {
                source,
                include_versions,
                remove_bucket,
            } => {
                self.purge(progress, source, include_versions, remove_bucket)
                    .await
            }
        }
    }

    async fn upload(
        &self,
        progress: &Progress<'_>,
        target: Location,
        paths: Vec<PathBuf>,
        overwrite: bool,
    ) -> Result<()> {
        let client = self.connections.session(target.connection)?.s3;
        progress.set_total(paths.len())?;
        for path in paths {
            let item = path.to_string_lossy();
            if !progress.begin(&item)? {
                break;
            }
            let result = upload_file(&client, &target, &path, overwrite).await;
            progress.record(&item, result)?;
        }
        Ok(())
    }

    async fn download(
        &self,
        progress: &Progress<'_>,
        connection: Uuid,
        items: Vec<ObjectId>,
        target: &Path,
        directory: bool,
    ) -> Result<()> {
        if !directory && items.len() != 1 {
            return Err(AppError::Invalid(
                "Choose a folder for multiple downloads.".into(),
            ));
        }
        let client = self.connections.session(connection)?.s3;
        progress.set_total(items.len())?;
        for object in items {
            if !progress.begin(&object.key)? {
                break;
            }
            let result = download_file(&client, &object, target, directory).await;
            progress.record(&object.key, result)?;
        }
        Ok(())
    }

    async fn archive(
        &self,
        progress: &Progress<'_>,
        connection: Uuid,
        items: Vec<ObjectId>,
        target: &Path,
    ) -> Result<()> {
        let client = self.connections.session(connection)?.s3;
        let mut names = HashSet::new();
        let (temporary, mut writer) = files::archive_writer(target)?;
        progress.set_total(items.len())?;
        for object in items {
            if !progress.begin(&object.key)? {
                return Ok(());
            }
            if !names.insert(object.key.clone()) {
                progress.record(
                    &object.key,
                    Err(AppError::Invalid(
                        "This ZIP already contains an entry with the same key.".into(),
                    )),
                )?;
                continue;
            }
            let result = append_archive(&client, &object, &mut writer).await;
            progress.record(&object.key, result)?;
        }
        if !progress.has_success()? {
            return Ok(());
        }
        writer
            .finish()
            .map_err(|error| AppError::File(error.to_string()))?
            .sync_all()?;
        temporary.persist(target).map_err(|error| error.error)?;
        Ok(())
    }

    async fn change_objects(
        &self,
        progress: &Progress<'_>,
        connection: Uuid,
        items: Vec<ObjectId>,
        tags: Option<Vec<Tag>>,
    ) -> Result<()> {
        let client = self.connections.session(connection)?.s3;
        progress.set_total(items.len())?;
        for object in items {
            if !progress.begin(&object.key)? {
                break;
            }
            let change = match &tags {
                None => ObjectChange::Delete,
                Some(tags) => ObjectChange::SetTags {
                    tags: tags.to_vec(),
                },
            };
            progress.record(
                &object.key,
                ObjectService::change(&client, &object, change).await,
            )?;
        }
        Ok(())
    }

    async fn copy(&self, progress: &Progress<'_>, plan: CopyPlan) -> Result<()> {
        let source = self.connections.session(plan.source.connection)?;
        let target = self.connections.session(plan.target.connection)?;
        progress.set_total(plan.objects.len())?;
        for object in &plan.objects {
            if !progress.begin(&object.key)? {
                break;
            }
            let result = copy_item(&source.s3, &target.s3, object, &plan).await;
            progress.record(&object.key, result)?;
        }
        Ok(())
    }

    async fn purge(
        &self,
        progress: &Progress<'_>,
        source: Location,
        include_versions: bool,
        remove_bucket: bool,
    ) -> Result<()> {
        if remove_bucket && (!source.prefix.is_empty() || !include_versions) {
            return Err(AppError::Invalid(
                "Removing a bucket requires its root and all versions.".into(),
            ));
        }
        let client = self.connections.session(source.connection)?.s3;
        let items = purge_items(&client, &source, include_versions).await?;
        progress.set_total(items.len() + usize::from(remove_bucket))?;
        for object in items {
            if !progress.begin(&object.key)? {
                return Ok(());
            }
            progress.record(&object.key, ObjectService::delete(&client, &object).await)?;
        }
        if remove_bucket && progress.can_remove_bucket()? && progress.begin(&source.bucket)? {
            progress.record(
                &source.bucket,
                crate::bucket::service::BucketService::delete(
                    &self.connections.session(source.connection)?,
                    &source.bucket,
                )
                .await,
            )?;
        }
        Ok(())
    }
}

impl Progress<'_> {
    fn set_total(&self, total: usize) -> Result<()> {
        self.jobs.update(self.id, |job| job.total = total)
    }

    fn begin(&self, item: &str) -> Result<bool> {
        self.jobs.update(self.id, |job| {
            job.current = Some(item.into());
            !job.cancel_requested
        })
    }

    fn record(&self, item: &str, result: Result<()>) -> Result<()> {
        self.jobs
            .update(self.id, |job| update_progress(job, item, result))
    }

    fn has_success(&self) -> Result<bool> {
        self.jobs.update(self.id, |job| job.succeeded > 0)
    }

    fn can_remove_bucket(&self) -> Result<bool> {
        self.jobs.update(self.id, |job| job.failed == 0)
    }

    fn finish(&self, result: Result<()>) -> Result<()> {
        self.jobs.update(self.id, |job| {
            if let Err(error) = result {
                update_progress(job, "Operation", Err(error));
            }
            job.status = if job.cancel_requested && job.succeeded + job.failed < job.total {
                JobStatus::Cancelled
            } else if job.failed > 0 {
                JobStatus::Failed
            } else {
                JobStatus::Completed
            };
            job.finished_at = Some(Utc::now());
            job.current = None;
        })?;
        self.jobs.checkpoint()
    }
}

fn update_progress(job: &mut Job, item: &str, result: Result<()>) {
    match result {
        Ok(()) => job.succeeded += 1,
        Err(error) => {
            job.failed += 1;
            if job.failures.len() < 200 {
                job.failures.push(JobFailure {
                    item: item.into(),
                    message: error.to_string(),
                });
            }
        }
    }
}

async fn upload_file(
    client: &Client,
    target: &Location,
    path: &Path,
    overwrite: bool,
) -> Result<()> {
    let key = format!("{}{}", target.prefix, files::path_name(path)?);
    let content_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    files::upload(
        client,
        &target.bucket,
        &key,
        path,
        UploadOptions {
            content_type,
            overwrite,
            ..UploadOptions::default()
        },
    )
    .await?;
    Ok(())
}

async fn download_file(
    client: &Client,
    object: &ObjectId,
    target: &Path,
    directory: bool,
) -> Result<()> {
    let path = files::download_target(target, directory, object)?;
    files::download(client, object, &path, !directory).await
}

async fn append_archive(
    client: &Client,
    object: &ObjectId,
    writer: &mut zip::ZipWriter<std::fs::File>,
) -> Result<()> {
    let mut temporary = NamedTempFile::new()?;
    files::stream_to_file(client, object, &mut temporary, None).await?;
    files::archive_add(writer, &object.key, &mut temporary.reopen()?)
}

async fn purge_items(
    client: &Client,
    source: &Location,
    include_versions: bool,
) -> Result<Vec<ObjectId>> {
    if include_versions {
        return inventory::all_versions(client, source).await;
    }
    Ok(inventory::objects(client, source)
        .await?
        .into_iter()
        .map(|row| object_id(source, row.key))
        .collect())
}

fn object_id(location: &Location, key: String) -> ObjectId {
    ObjectId {
        bucket: location.bucket.clone(),
        key,
        version_id: None,
    }
}

fn copy_target(object: &ObjectId, plan: &CopyPlan) -> Result<String> {
    if object.bucket != plan.source.bucket {
        return Err(AppError::Invalid(
            "The selected file belongs to a different source bucket.".into(),
        ));
    }
    let relative = object
        .key
        .strip_prefix(&plan.source.prefix)
        .ok_or_else(|| {
            AppError::Invalid("The selected file is outside the source prefix.".into())
        })?;
    let key = format!("{}{relative}", plan.target.prefix);
    if key.is_empty()
        || (plan.source.connection == plan.target.connection
            && object.bucket == plan.target.bucket
            && object.key == key)
    {
        return Err(AppError::Invalid(
            "Choose a different destination key.".into(),
        ));
    }
    Ok(key)
}

async fn copy_item(
    source: &Client,
    target: &Client,
    object: &ObjectId,
    plan: &CopyPlan,
) -> Result<()> {
    let key = copy_target(object, plan)?;
    let source_info = objects::head(source, object).await?;
    let length = source_info.content_length().unwrap_or_default();
    let etag = source_info.e_tag.clone();
    let receipt = if plan.overwrite
        && plan.source.connection == plan.target.connection
        && length <= 5 * 1024 * 1024 * 1024
    {
        copy_inside_endpoint(target, object, &plan.target.bucket, &key, etag.clone()).await?
    } else {
        copy_via_disk(
            source,
            target,
            object,
            &plan.target,
            &key,
            source_info,
            plan.overwrite,
        )
        .await?
    };
    let destination = ObjectId {
        bucket: plan.target.bucket.clone(),
        key,
        version_id: receipt.version_id,
    };
    verify_destination(target, &destination, length, receipt.etag.as_deref()).await?;
    if plan.move_source {
        delete_moved_source(source, object, etag).await?;
    }
    Ok(())
}

async fn copy_inside_endpoint(
    client: &Client,
    object: &ObjectId,
    bucket: &str,
    key: &str,
    etag: Option<String>,
) -> Result<UploadReceipt> {
    let copied = client
        .copy_object()
        .bucket(bucket)
        .key(key)
        .copy_source(objects::copy_source(object))
        .set_copy_source_if_match(etag)
        .send()
        .await
        .map_err(storage_error)?;
    Ok(UploadReceipt {
        etag: copied.copy_object_result.and_then(|result| result.e_tag),
        version_id: copied.version_id,
    })
}

async fn copy_via_disk(
    source: &Client,
    target: &Client,
    object: &ObjectId,
    destination: &Location,
    key: &str,
    source_info: aws_sdk_s3::operation::head_object::HeadObjectOutput,
    overwrite: bool,
) -> Result<UploadReceipt> {
    let mut file = NamedTempFile::new()?;
    let size = files::stream_to_file(source, object, &mut file, source_info.e_tag).await?;
    if Some(size as i64) != source_info.content_length {
        return Err(AppError::Storage(
            "Downloaded size did not match the source.".into(),
        ));
    }
    let content_type = source_info
        .content_type
        .unwrap_or_else(|| "application/octet-stream".into());
    files::upload(
        target,
        &destination.bucket,
        key,
        file.path(),
        UploadOptions {
            content_type,
            overwrite,
            metadata: source_info.metadata,
            cache_control: source_info.cache_control,
            content_disposition: source_info.content_disposition,
            content_encoding: source_info.content_encoding,
            content_language: source_info.content_language,
        },
    )
    .await
}

async fn verify_destination(
    client: &Client,
    object: &ObjectId,
    size: i64,
    etag: Option<&str>,
) -> Result<()> {
    let destination = objects::head(client, object).await?;
    if destination.content_length() != Some(size) {
        return Err(AppError::Storage(
            "Destination size did not match the source.".into(),
        ));
    }
    if etag.is_some_and(|received| {
        destination.e_tag().map(|value| value.trim_matches('"')) != Some(received.trim_matches('"'))
    }) {
        return Err(AppError::Storage(
            "The destination changed during verification.".into(),
        ));
    }
    Ok(())
}

async fn delete_moved_source(
    client: &Client,
    object: &ObjectId,
    etag: Option<String>,
) -> Result<()> {
    let etag = objects::required_id(etag.as_deref(), "source ETag")?;
    if objects::head(client, object).await?.e_tag() != Some(etag.as_str()) {
        return Err(AppError::Storage(
            "The source changed during the move. The destination was copied and the source was kept.".into(),
        ));
    }
    client
        .delete_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .if_match(etag)
        .send()
        .await
        .map_err(storage_error)?;
    Ok(())
}
