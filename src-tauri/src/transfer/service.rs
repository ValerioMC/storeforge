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
    common::error::{AppError, Result},
    connection::{
        model::{IdentityKind, Workspace},
        service::{ConnectionService, Session},
    },
    object::{
        model::{Location, ObjectChange, ObjectId, Tag},
        repository as objects,
        service::ObjectService,
    },
};

use super::{
    batch,
    files::{self, UploadOptions, UploadReceipt},
    inventory,
    maintenance::{
        self, BucketResult, BucketStatus, IntegrityRequest, MaintenanceOptions, MaintenanceReport,
        PurgePreview,
    },
    model::{BulkAttributes, Job, JobFailure, JobStatus, Transfer, VersionPlanItem, VersionRule},
    repository::JobRepository,
    security::{AccessInput, OperationAccess, UploadAccess, transfer_error, write_client},
    sts::RoleScope,
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
    rename_to: Option<String>,
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

    pub fn start(&self, transfer: Transfer, access: Option<AccessInput>) -> Result<Uuid> {
        if access.is_some()
            && !matches!(
                &transfer,
                Transfer::Upload { .. } | Transfer::Download { .. }
            )
        {
            return Err(AppError::Invalid(
                "Transfer credentials apply to uploads and downloads.".into(),
            ));
        }
        let access = OperationAccess::parse(
            access.unwrap_or_default(),
            matches!(&transfer, Transfer::Upload { .. }),
        )?;
        let maintenance = self.prepare_maintenance(&transfer)?;
        let id = Uuid::new_v4();
        let label = transfer.label().into();
        self.jobs.insert(Job {
            id,
            label,
            status: JobStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            total: maintenance
                .as_ref()
                .map_or(0, |report| report.buckets.len()),
            succeeded: 0,
            failed: 0,
            failures: Vec::new(),
            successful_items: Vec::new(),
            bytes_processed: 0,
            current_bytes: 0,
            current: None,
            cancel_requested: false,
            maintenance,
        })?;
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            let progress = Progress {
                id,
                jobs: &service.jobs,
            };
            let result = service.run(&progress, transfer, &access).await;
            if let Err(error) = progress.finish(result) {
                tracing::error!(%error, %id, "Could not persist transfer finish");
            }
        });
        Ok(id)
    }

    fn prepare_maintenance(&self, transfer: &Transfer) -> Result<Option<MaintenanceReport>> {
        let (options, selection) = match transfer {
            Transfer::Integrity { request } => {
                request.validate()?;
                self.maintenance_session(request.selection.connection)?;
                (MaintenanceOptions::from(request), &request.selection)
            }
            Transfer::Purge {
                preview,
                confirmation,
            } => {
                let session = self.maintenance_session(preview.request.selection.connection)?;
                preview.validate(&session, confirmation)?;
                (
                    MaintenanceOptions::from(&preview.request),
                    &preview.request.selection,
                )
            }
            _ => return Ok(None),
        };
        Ok(Some(MaintenanceReport {
            connection: selection.connection,
            options,
            buckets: selection
                .targets
                .iter()
                .cloned()
                .map(BucketResult::new)
                .collect(),
        }))
    }

    fn maintenance_session(&self, connection: Uuid) -> Result<Session> {
        self.connections
            .session_for(connection, Workspace::Manager, IdentityKind::S3)
    }

    async fn run(
        &self,
        progress: &Progress<'_>,
        transfer: Transfer,
        access: &OperationAccess,
    ) -> Result<()> {
        match transfer {
            Transfer::Upload {
                target,
                paths,
                overwrite,
            } => {
                self.upload(progress, target, paths, overwrite, access)
                    .await
            }
            Transfer::Download {
                connection,
                objects,
                target,
                directory,
            } => {
                self.download(progress, connection, objects, &target, directory, access)
                    .await
            }
            Transfer::Archive {
                connection,
                objects,
                folders,
                prefix,
                bucket,
                target,
            } => {
                let client = self.connections.session(connection)?.s3;
                let bucket = bucket
                    .or_else(|| objects.first().map(|object| object.bucket.clone()))
                    .ok_or_else(|| {
                        AppError::Invalid("Select objects or folders to archive.".into())
                    })?;
                let source = Location {
                    connection,
                    bucket,
                    prefix,
                };
                let mut objects =
                    batch::expand_selection(&client, &source, objects, folders).await?;
                objects.retain(|object| object.key != source.prefix);
                self.archive(progress, &client, objects, &source.prefix, &target)
                    .await
            }
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
                folders,
                rename_to,
                target,
                move_source,
                overwrite,
            } => {
                let source_session = self.connections.session(source.connection)?;
                let target_session = self.connections.session(target.connection)?;
                let same_endpoint = source_session.profile.endpoint
                    == target_session.profile.endpoint
                    && source_session.context.tenant == target_session.context.tenant;
                validate_copy_selection(
                    same_endpoint,
                    &source,
                    &target,
                    &objects,
                    &folders,
                    rename_to.as_deref(),
                )?;
                let objects =
                    batch::expand_selection(&source_session.s3, &source, objects, folders).await?;
                self.copy(
                    progress,
                    &source_session,
                    &target_session,
                    CopyPlan {
                        source,
                        target,
                        objects,
                        move_source,
                        rename_to,
                        overwrite,
                    },
                )
                .await
            }
            Transfer::Attributes {
                source,
                objects,
                folders,
                update,
            } => {
                self.attributes(progress, source, objects, folders, *update)
                    .await
            }
            Transfer::ApplyVersionPlan {
                source,
                rule,
                items,
            } => self.apply_version_plan(progress, source, rule, items).await,
            Transfer::Migrate {
                source,
                target,
                overwrite,
            } => {
                let source_session = self.connections.session(source.connection)?;
                let target_session = self.connections.session(target.connection)?;
                let rows = inventory::objects(&source_session.s3, &source).await?;
                let objects = rows
                    .into_iter()
                    .map(|row| object_id(&source, row.key))
                    .collect();
                self.copy(
                    progress,
                    &source_session,
                    &target_session,
                    CopyPlan {
                        source,
                        target,
                        objects,
                        move_source: false,
                        rename_to: None,
                        overwrite,
                    },
                )
                .await
            }
            Transfer::Integrity { request } => self.integrity(progress, request).await,
            Transfer::Purge {
                preview,
                confirmation,
            } => self.purge(progress, preview, confirmation).await,
        }
    }

    async fn upload(
        &self,
        progress: &Progress<'_>,
        target: Location,
        paths: Vec<PathBuf>,
        overwrite: bool,
        access: &OperationAccess,
    ) -> Result<()> {
        let session = self.connections.session(target.connection)?;
        let keys = if access.role_arn.is_some() {
            paths
                .iter()
                .map(|path| upload_key(&target, path))
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let client = access
            .client(
                &session,
                RoleScope::Write {
                    bucket: &target.bucket,
                    keys: &keys,
                },
            )
            .await?;
        progress.set_total(paths.len())?;
        for path in paths {
            let item = path.to_string_lossy();
            if !progress.begin(&item)? {
                break;
            }
            let result = upload_file(
                client.client()?,
                &target,
                &path,
                overwrite,
                access.upload_access(),
                progress,
            )
            .await;
            progress.record_bytes(&item, result)?;
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
        access: &OperationAccess,
    ) -> Result<()> {
        if !directory && items.len() != 1 {
            return Err(AppError::Invalid(
                "Choose a folder for multiple downloads.".into(),
            ));
        }
        let session = self.connections.session(connection)?;
        let client = access.client(&session, RoleScope::Read(&items)).await?;
        progress.set_total(items.len())?;
        for object in items {
            if !progress.begin(&object.key)? {
                break;
            }
            let result = download_file(
                client.client()?,
                &object,
                target,
                directory,
                access,
                progress,
            )
            .await;
            progress.record_bytes(&object.key, result)?;
        }
        Ok(())
    }

    async fn archive(
        &self,
        progress: &Progress<'_>,
        client: &Client,
        items: Vec<ObjectId>,
        prefix: &str,
        target: &Path,
    ) -> Result<()> {
        let mut names = files::ArchiveNames::default();
        let (temporary, mut writer) = files::archive_writer(target)?;
        progress.set_total(items.len())?;
        for object in items {
            if !progress.begin(&object.key)? {
                break;
            }
            let result =
                archive_item(client, &object, prefix, &mut writer, &mut names, progress).await;
            progress.record_bytes(&object.key, result)?;
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

    async fn copy(
        &self,
        progress: &Progress<'_>,
        source: &Session,
        target: &Session,
        plan: CopyPlan,
    ) -> Result<()> {
        let mut destinations = HashSet::new();
        progress.set_total(plan.objects.len())?;
        for object in &plan.objects {
            if !progress.begin(&object.key)? {
                break;
            }
            let result = match copy_target(object, &plan) {
                Ok(key) if destinations.insert(key.clone()) => {
                    copy_item(&source.s3, &target.s3, object, &plan, key, progress).await
                }
                Ok(_) => Err(AppError::Invalid(
                    "Multiple selections map to this destination key.".into(),
                )),
                Err(error) => Err(error),
            };
            progress.record_bytes(&object.key, result)?;
        }
        Ok(())
    }

    async fn attributes(
        &self,
        progress: &Progress<'_>,
        source: Location,
        objects: Vec<ObjectId>,
        folders: Vec<String>,
        update: BulkAttributes,
    ) -> Result<()> {
        let client = self.connections.session(source.connection)?.s3;
        let objects = batch::expand_selection(&client, &source, objects, folders).await?;
        progress.set_total(objects.len())?;
        for object in objects {
            if !progress.begin(&object.key)? {
                break;
            }
            progress.record(
                &object.key,
                apply_attributes(&client, &object, &update).await,
            )?;
        }
        Ok(())
    }

    async fn apply_version_plan(
        &self,
        progress: &Progress<'_>,
        source: Location,
        rule: VersionRule,
        items: Vec<VersionPlanItem>,
    ) -> Result<()> {
        if items
            .iter()
            .any(|item| !item.key.starts_with(&source.prefix))
        {
            return Err(AppError::Invalid(
                "The reviewed plan contains a key outside its prefix.".into(),
            ));
        }
        let client = self.connections.session(source.connection)?.s3;
        let mut seen = HashSet::new();
        let items: Vec<_> = items.iter().filter(|item| seen.insert(*item)).collect();
        progress.set_total(items.len())?;
        for item in items {
            if !progress.begin(&item.key)? {
                break;
            }
            progress.record(
                &item.key,
                batch::apply_version(&client, &source, &rule, item).await,
            )?;
        }
        Ok(())
    }

    async fn integrity(&self, progress: &Progress<'_>, request: IntegrityRequest) -> Result<()> {
        let session = self.maintenance_session(request.selection.connection)?;
        for (index, scope) in request.selection.targets.iter().cloned().enumerate() {
            if !progress.begin(&scope.label())? {
                break;
            }
            let report = maintenance::run_integrity(&session, scope, &request, |report| {
                progress.publish_bucket(index, report)
            })
            .await?;
            progress.finish_bucket(index, &report)?;
        }
        Ok(())
    }

    async fn purge(
        &self,
        progress: &Progress<'_>,
        preview: PurgePreview,
        confirmation: String,
    ) -> Result<()> {
        let session = self.maintenance_session(preview.request.selection.connection)?;
        preview.validate(&session, &confirmation)?;
        for (index, target) in preview.buckets.iter().enumerate() {
            if !progress.begin(&target.scope.label())? {
                break;
            }
            let report = maintenance::run_purge(&session, target, &preview.request, |report| {
                progress.publish_bucket(index, report)
            })
            .await?;
            progress.finish_bucket(index, &report)?;
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
            job.current_bytes = 0;
            !job.cancel_requested
        })
    }

    fn record(&self, item: &str, result: Result<()>) -> Result<()> {
        self.jobs
            .update(self.id, |job| update_progress(job, item, result))
    }

    fn update_bytes(&self, bytes: u64) -> Result<()> {
        self.jobs.update(self.id, |job| job.current_bytes = bytes)
    }

    fn record_bytes(&self, item: &str, result: Result<u64>) -> Result<()> {
        self.jobs.update(self.id, |job| {
            let completed = result.map(|bytes| {
                job.bytes_processed = job.bytes_processed.saturating_add(bytes);
            });
            job.current_bytes = 0;
            update_progress(job, item, completed);
        })
    }

    fn has_success(&self) -> Result<bool> {
        self.jobs.update(self.id, |job| job.succeeded > 0)
    }

    fn publish_bucket(&self, index: usize, report: &BucketResult) -> Result<bool> {
        self.jobs.update(self.id, |job| {
            let bucket = job
                .maintenance
                .as_mut()
                .and_then(|maintenance| maintenance.buckets.get_mut(index))
                .ok_or_else(|| AppError::Missing("The bucket job scope is unavailable.".into()))?;
            *bucket = report.clone();
            Ok(!job.cancel_requested)
        })?
    }

    fn finish_bucket(&self, index: usize, report: &BucketResult) -> Result<()> {
        self.publish_bucket(index, report)?;
        if report.status != BucketStatus::Cancelled {
            let outcome = if report.failed == 0 {
                Ok(())
            } else {
                Err(AppError::Storage(format!(
                    "{} bucket operation failure(s).",
                    report.failed
                )))
            };
            self.record(&report.scope.label(), outcome)?;
        }
        self.jobs.checkpoint()
    }

    fn finish(&self, result: Result<()>) -> Result<()> {
        self.jobs.update(self.id, |job| {
            if let Err(error) = result {
                update_progress(job, "Operation", Err(error));
                job.total = job.total.max(job.succeeded + job.failed);
            }
            job.status = if job.cancel_requested
                && (job.total == 0 || job.succeeded + job.failed < job.total)
            {
                JobStatus::Cancelled
            } else if job.failed > 0 {
                JobStatus::Failed
            } else {
                JobStatus::Completed
            };
            job.finished_at = Some(Utc::now());
            job.current = None;
            job.current_bytes = 0;
        })?;
        self.jobs.checkpoint()
    }
}

fn update_progress(job: &mut Job, item: &str, result: Result<()>) {
    match result {
        Ok(()) => {
            job.succeeded += 1;
            job.successful_items.push(item.into());
        }
        Err(error) => {
            job.failed += 1;
            job.failures.push(JobFailure {
                item: item.into(),
                message: error.to_string(),
            });
        }
    }
}

async fn upload_file(
    client: &Client,
    target: &Location,
    path: &Path,
    overwrite: bool,
    access: UploadAccess<'_>,
    progress: &Progress<'_>,
) -> Result<u64> {
    let bytes = tokio::fs::metadata(path).await?.len();
    let key = upload_key(target, path)?;
    let content_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    files::upload_observed(
        client,
        &target.bucket,
        &key,
        path,
        UploadOptions {
            content_type,
            overwrite,
            access,
            ..UploadOptions::default()
        },
        &|bytes| progress.update_bytes(bytes),
    )
    .await?;
    Ok(bytes)
}

fn upload_key(target: &Location, path: &Path) -> Result<String> {
    Ok(format!("{}{}", target.prefix, files::path_name(path)?))
}

async fn download_file(
    client: &Client,
    object: &ObjectId,
    target: &Path,
    directory: bool,
    access: &OperationAccess,
    progress: &Progress<'_>,
) -> Result<u64> {
    let path = files::download_target(target, directory, object)?;
    files::download(
        client,
        object,
        &path,
        !directory,
        access.read_access(),
        &|bytes| progress.update_bytes(bytes),
    )
    .await
}

async fn archive_item(
    client: &Client,
    object: &ObjectId,
    prefix: &str,
    writer: &mut zip::ZipWriter<std::fs::File>,
    names: &mut files::ArchiveNames,
    progress: &Progress<'_>,
) -> Result<u64> {
    let relative = object
        .key
        .strip_prefix(prefix)
        .ok_or_else(|| AppError::Invalid("This key is outside the archive prefix.".into()))?;
    if relative.ends_with('/') {
        if objects::head(client, object).await?.content_length() != Some(0) {
            return Err(AppError::Invalid(
                "A nonempty object ending with a slash cannot be archived as a directory.".into(),
            ));
        }
        files::archive_add_directory(writer, &names.reserve(relative)?)?;
        return Ok(0);
    }
    let name = names.reserve(relative)?;
    let mut temporary = NamedTempFile::new()?;
    let size = files::stream_to_file(
        client,
        object,
        &mut temporary,
        None,
        Default::default(),
        &|bytes| progress.update_bytes(bytes),
    )
    .await?;
    files::archive_add(writer, &name, &mut temporary.reopen()?)?;
    Ok(size)
}

fn object_id(location: &Location, key: String) -> ObjectId {
    ObjectId {
        bucket: location.bucket.clone(),
        key,
        version_id: None,
    }
}

async fn apply_attributes(
    client: &Client,
    object: &ObjectId,
    update: &BulkAttributes,
) -> Result<()> {
    let mut failures = Vec::new();
    if update.metadata.is_some() || update.storage_class.is_some() {
        ObjectService::update_copy_attributes(
            client,
            object,
            update.metadata.clone(),
            update.storage_class.as_deref(),
        )
        .await?;
    }
    let mut changes = Vec::new();
    if let Some(tags) = &update.tags {
        changes.push(("Tags", ObjectChange::SetTags { tags: tags.clone() }));
    }
    if let Some(acl) = &update.acl {
        changes.push(("ACL", ObjectChange::SetAcl { acl: acl.clone() }));
    }
    if let Some(enabled) = update.legal_hold {
        changes.push(("Legal hold", ObjectChange::LegalHold { enabled }));
    }
    for (label, change) in changes {
        if let Err(error) = ObjectService::change(client, object, change).await {
            failures.push(format!("{label}: {error}"));
        }
    }
    if let Some(retention) = &update.retention
        && let Err(error) = ObjectService::set_retention(
            client,
            object,
            retention.mode,
            &retention.until,
            retention.bypass_governance,
        )
        .await
    {
        failures.push(format!("Retention: {error}"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::Storage(failures.join(" · ")))
    }
}

fn validate_copy_selection(
    same_endpoint: bool,
    source: &Location,
    target: &Location,
    objects: &[ObjectId],
    folders: &[String],
    rename_to: Option<&str>,
) -> Result<()> {
    if rename_to
        .is_some_and(|name| name.is_empty() || name.contains('/') || matches!(name, "." | ".."))
        || (rename_to.is_some() && (!folders.is_empty() || objects.len() != 1))
    {
        return Err(AppError::Invalid(
            "Rename requires one object and a single destination name.".into(),
        ));
    }
    if same_endpoint
        && source.bucket == target.bucket
        && folders
            .iter()
            .any(|folder| target.prefix.starts_with(folder))
    {
        return Err(AppError::Invalid(
            "Choose a destination outside the selected folders.".into(),
        ));
    }
    Ok(())
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
    let key = format!(
        "{}{}",
        plan.target.prefix,
        plan.rename_to.as_deref().unwrap_or(relative)
    );
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
    key: String,
    progress: &Progress<'_>,
) -> Result<u64> {
    let source_info = objects::head(source, object).await?;
    let length = source_info
        .content_length()
        .filter(|length| *length >= 0)
        .ok_or_else(|| AppError::Storage("The endpoint did not report the source size.".into()))?;
    let etag = source_info.e_tag.clone();
    if plan.move_source {
        objects::required_id(etag.as_deref(), "source ETag")?;
    }
    let receipt = if plan.overwrite
        && plan.source.connection == plan.target.connection
        && length <= 5 * 1024 * 1024 * 1024
    {
        copy_inside_endpoint(target, object, &plan.target.bucket, &key, etag.clone()).await?
    } else {
        copy_via_disk(source, target, object, &key, source_info, plan, progress).await?
    };
    let destination = ObjectId {
        bucket: plan.target.bucket.clone(),
        key,
        version_id: receipt.version_id,
    };
    let destination_etag = if plan.move_source {
        Some(objects::required_id(
            receipt.etag.as_deref(),
            "destination ETag",
        )?)
    } else {
        receipt.etag
    };
    verify_destination(target, &destination, length, destination_etag.as_deref()).await?;
    if plan.move_source {
        delete_moved_source(source, object, etag).await?;
    }
    Ok(length as u64)
}

async fn copy_inside_endpoint(
    client: &Client,
    object: &ObjectId,
    bucket: &str,
    key: &str,
    etag: Option<String>,
) -> Result<UploadReceipt> {
    let copied = write_client(client)
        .copy_object()
        .bucket(bucket)
        .key(key)
        .copy_source(objects::copy_source(object))
        .set_copy_source_if_match(etag)
        .send()
        .await
        .map_err(|error| transfer_error(error, false, true))?;
    Ok(UploadReceipt {
        etag: copied.copy_object_result.and_then(|result| result.e_tag),
        version_id: copied.version_id,
    })
}

async fn copy_via_disk(
    source: &Client,
    target: &Client,
    object: &ObjectId,
    key: &str,
    source_info: aws_sdk_s3::operation::head_object::HeadObjectOutput,
    plan: &CopyPlan,
    progress: &Progress<'_>,
) -> Result<UploadReceipt> {
    let mut file = NamedTempFile::new()?;
    let size = files::stream_to_file(
        source,
        object,
        &mut file,
        source_info.e_tag,
        Default::default(),
        &|bytes| progress.update_bytes(bytes),
    )
    .await?;
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
        &plan.target.bucket,
        key,
        file.path(),
        UploadOptions {
            content_type,
            overwrite: plan.overwrite,
            metadata: source_info.metadata,
            cache_control: source_info.cache_control,
            content_disposition: source_info.content_disposition,
            content_encoding: source_info.content_encoding,
            content_language: source_info.content_language,
            ..UploadOptions::default()
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
    write_client(client)
        .delete_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .if_match(etag)
        .send()
        .await
        .map_err(|error| transfer_error(error, false, true))?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/transfer/service_batch.rs"]
mod tests;
