use aws_sdk_s3::{Client, error::ProvideErrorMetadata, primitives::ByteStream};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    bucket::{
        model::{Configuration, ConfigurationChange},
        repository as buckets,
        service::BucketService,
    },
    common::error::{AppError, Result},
    connection::{model::Provider, service::Session},
    object::{
        model::{Location, ObjectChange, ObjectId, Tag},
        repository as objects,
        service::ObjectService,
    },
};

use super::{
    model::{Draft, Effect, Mapping, Method, Migration, PairPlan, Side},
    ownership,
    repository::MigrationRepository,
};

use crate::transfer::{
    inventory,
    security::{transfer_error, write_client},
    sts,
};

const PROBE_KEY: &str = "storeforge-preflight";
const MULTIPART_KEY: &str = "storeforge-multipart";
const PROBE_BODY: &[u8] = b"Storeforge migration probe";

pub const COPIED_SETTINGS: &[Configuration] = &[
    Configuration::Versioning,
    Configuration::Lifecycle,
    Configuration::Cors,
    Configuration::Encryption,
    Configuration::PublicAccessBlock,
    Configuration::Tagging,
];
pub const OMITTED_SETTINGS: &[Configuration] = &[
    Configuration::Acl,
    Configuration::Policy,
    Configuration::OwnershipControls,
    Configuration::Logging,
    Configuration::Notification,
    Configuration::Replication,
    Configuration::Website,
    Configuration::RequestPayment,
    Configuration::ObjectLock,
];

pub struct PairCheck {
    pub plan: PairPlan,
    pub sample: Option<ObjectId>,
    pub settings: Vec<(Configuration, String)>,
}

pub async fn read_pair(
    source: &Session,
    target: &Session,
    draft: &Draft,
    mapping: &Mapping,
) -> PairCheck {
    let mut check = inspect_source(source, draft, mapping).await;
    if let Err(error) = require_absent(&target.s3, &mapping.target).await {
        check.plan.issues.push(error.to_string());
    }
    check
}

pub async fn inspect_source(source: &Session, draft: &Draft, mapping: &Mapping) -> PairCheck {
    let mut check = PairCheck {
        plan: PairPlan {
            mapping: mapping.clone(),
            sample: None,
            settings_copied: Vec::new(),
            settings_omitted: OMITTED_SETTINGS.to_vec(),
            issues: Vec::new(),
        },
        sample: None,
        settings: Vec::new(),
    };
    match sample_object(source, draft, &mapping.source).await {
        Ok(None) if draft.method == Method::Storage => check.plan.issues.push(
            "Storage-side copy needs a representative source object. Choose streamed copy.".into(),
        ),
        Ok(sample) => {
            check.plan.sample = sample.as_ref().map(|object| object.key.clone());
            check.sample = sample;
        }
        Err(issue) => check.plan.issues.push(issue.into()),
    }
    if policy(&source.s3, &mapping.source).await.is_err() {
        check
            .plan
            .issues
            .push("Could not read the source bucket policy.".into());
    }
    check_settings(source, draft, &mapping.source, &mut check).await;
    check
}

async fn sample_object(
    source: &Session,
    draft: &Draft,
    bucket: &str,
) -> std::result::Result<Option<ObjectId>, &'static str> {
    let sample = if draft.history {
        source
            .s3
            .list_object_versions()
            .bucket(bucket)
            .max_keys(32)
            .send()
            .await
            .map_err(|_| "Could not list source object versions.")?
            .versions()
            .first()
            .map(|version| {
                let key = version
                    .key()
                    .ok_or("The provider omitted the sample key.")?;
                let id = version
                    .version_id()
                    .ok_or("The provider omitted the sample version.")?;
                Ok::<ObjectId, &'static str>(ObjectId {
                    bucket: bucket.into(),
                    key: key.into(),
                    version_id: Some(id.into()),
                })
            })
            .transpose()?
    } else {
        source
            .s3
            .list_objects_v2()
            .bucket(bucket)
            .max_keys(1)
            .send()
            .await
            .map_err(|_| "Could not list source objects.")?
            .contents()
            .first()
            .map(|row| {
                let key = row.key().ok_or("The provider omitted the sample key.")?;
                Ok::<ObjectId, &'static str>(ObjectId {
                    bucket: bucket.into(),
                    key: key.into(),
                    version_id: None,
                })
            })
            .transpose()?
    };
    if let Some(object) = &sample {
        let head = objects::head(&source.s3, object)
            .await
            .map_err(|_| "Could not read source object metadata.")?;
        if head.ssekms_key_id().is_some() || head.sse_customer_algorithm().is_some() {
            return Err("KMS and customer-encrypted objects are outside the migration perimeter.");
        }
        exact_tags(&source.s3, object)
            .await
            .map_err(|_| "Could not read exact source object tags.")?;
    }
    Ok(sample)
}

async fn check_settings(source: &Session, draft: &Draft, bucket: &str, check: &mut PairCheck) {
    use Configuration::*;
    let features = if draft.settings {
        [
            COPIED_SETTINGS,
            &[ObjectLock, Notification, Replication, Website],
        ]
        .concat()
    } else {
        vec![Versioning, ObjectLock, Encryption]
    };
    for feature in features {
        match buckets::configure(source, bucket, feature, ConfigurationChange::Read).await {
            Ok(document) => match setting_state(feature, &document, draft) {
                Ok(true) => {
                    check.plan.settings_copied.push(feature);
                    check.settings.push((feature, document));
                }
                Ok(false) => {}
                Err(issue) => check.plan.issues.push(issue),
            },
            Err(AppError::Missing(_)) => {}
            Err(_) => check.plan.issues.push(format!(
                "Could not inspect {} on the source bucket.",
                feature_name(feature)
            )),
        }
    }
    if draft.history && !check.plan.settings_copied.contains(&Versioning) {
        check.plan.settings_copied.push(Versioning);
    }
}

fn setting_state(
    feature: Configuration,
    document: &str,
    draft: &Draft,
) -> std::result::Result<bool, String> {
    use Configuration::*;
    let xml = xml_elements(document)
        .map_err(|_| format!("Invalid {} document.", feature_name(feature)))?;
    let has = |tag: &str| xml.iter().any(|element| element == tag);
    match feature {
        ObjectLock if has("ObjectLockEnabled") => Err("Object Lock is outside the migration perimeter.".into()),
        Encryption if has("KMSMasterKeyID") || document.contains("aws:kms") => Err("SSE-KMS is outside the migration perimeter.".into()),
        Notification if ["TopicConfiguration", "QueueConfiguration", "CloudFunctionConfiguration", "EventBridgeConfiguration"]
            .iter().any(|tag| has(tag)) => Err("Notification rules must be migrated separately. Disable configuration copy.".into()),
        Replication if has("Rule") => Err("Replication rules must be migrated separately. Disable configuration copy.".into()),
        Website if has("IndexDocument") || has("RedirectAllRequestsTo") => Err("Website hosting must be migrated separately. Disable configuration copy.".into()),
        Versioning if draft.settings && !draft.history && has("Status") => Err(
            "Copying versioning settings requires version history. Enable history or disable configuration copy.".into(),
        ),
        Versioning | ObjectLock | Notification | Replication | Website => Ok(false),
        Encryption => Ok(draft.settings && has("Rule")),
        Lifecycle => Ok(has("Rule")),
        Cors => Ok(has("CORSRule")),
        Tagging if xml.iter().filter(|element| element.as_str() == "Tag").count() >= 50
            || document.contains(ownership::RECEIPT_TAG) => Err(
                "Source bucket tags leave no slot for the migration ownership receipt.".into(),
            ),
        Tagging => Ok(has("Tag")),
        _ => Ok(draft.settings),
    }
}

fn xml_elements(document: &str) -> Result<Vec<String>> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(document);
    let mut elements = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                elements.push(event.local_name().as_ref().to_owned());
            }
            Ok(Event::Eof) if !elements.is_empty() => return Ok(elements),
            Ok(Event::Eof) | Err(_) => {
                return Err(AppError::Storage(
                    "Invalid provider configuration XML.".into(),
                ));
            }
            _ => {}
        }
    }
}

pub fn feature_name(configuration: Configuration) -> &'static str {
    use Configuration::*;
    match configuration {
        Acl => "ACL",
        Cors => "CORS",
        Encryption => "encryption",
        Lifecycle => "lifecycle",
        Logging => "logging",
        Notification => "notifications",
        ObjectLock => "Object Lock",
        OwnershipControls => "ownership controls",
        Policy => "bucket policy",
        PublicAccessBlock => "public access blocks",
        Replication => "replication",
        RequestPayment => "request payment",
        Tagging => "bucket tags",
        Versioning => "versioning",
        Website => "website hosting",
    }
}

pub async fn active_pair(
    repository: &MigrationRepository,
    record: &mut Migration,
    source: &Session,
    target: &Session,
    check: &PairCheck,
) -> Result<()> {
    let source_bucket = &check.plan.mapping.source;
    let principal = worker_principal(&record.draft, target).await?;
    protect(
        repository,
        record,
        source,
        Side::Source,
        source_bucket,
        PolicyAccess {
            delete_only: false,
            deny_writes: true,
            writer: None,
            read_grant: principal
                .as_deref()
                .filter(|_| record.draft.grant_source_read),
        },
    )
    .await?;
    if let Some(object) = &check.sample {
        verify_sample_read(source, object).await?;
    }
    let probe = format!("storeforge-check-{}", Uuid::new_v4().simple());
    let index = record.private.effects.len();
    record.private.effects.push(Effect::Probe {
        bucket: probe.clone(),
        created: false,
        upload: None,
    });
    repository.save(record)?;
    create_new_bucket(target, &probe).await?;
    if let Effect::Probe { created, .. } = &mut record.private.effects[index] {
        *created = true;
    }
    repository.save(record)?;
    ownership::probe_tagging(target, &probe).await?;
    if record.draft.protect_destination {
        protect(
            repository,
            record,
            target,
            Side::Target,
            &probe,
            PolicyAccess {
                delete_only: false,
                deny_writes: true,
                writer: principal.as_deref(),
                read_grant: None,
            },
        )
        .await?;
    }
    if record.draft.history {
        BucketService::finish_versioning(target, &probe, None)
            .await
            .map_err(|_| {
                AppError::Storage("Could not enable versioning on the temporary bucket.".into())
            })?;
    }
    if record.draft.method == Method::Storage {
        let sample = check
            .sample
            .as_ref()
            .ok_or_else(|| AppError::Invalid("The source sample disappeared.".into()))?;
        storage_copy(&target.s3, sample, &probe, PROBE_KEY).await?;
    } else {
        write_client(&target.s3)
            .put_object()
            .bucket(&probe)
            .key(PROBE_KEY)
            .body(ByteStream::from_static(PROBE_BODY))
            .send()
            .await
            .map_err(|_| AppError::Storage("Could not write to the temporary bucket.".into()))?;
    }
    verify_probe_object(target, &probe).await?;
    test_multipart(repository, record, target, &probe, index).await?;
    for (feature, document) in &check.settings {
        buckets::configure(
            target,
            &probe,
            *feature,
            ConfigurationChange::Write(document.clone()),
        )
        .await
        .map_err(|_| {
            AppError::Storage(format!(
                "Could not write {} on the temporary bucket.",
                feature_name(*feature)
            ))
        })?;
    }
    Ok(())
}

pub async fn worker_principal(draft: &Draft, target: &Session) -> Result<Option<String>> {
    if !draft.protect_destination && !draft.grant_source_read {
        return Ok(None);
    }
    sts::caller_principal(target).await.map(Some)
}

pub async fn verify_sample_read(source: &Session, object: &ObjectId) -> Result<()> {
    let head = objects::head(&source.s3, object)
        .await
        .map_err(|_| AppError::Storage("Could not recheck source object metadata.".into()))?;
    let mut body = source
        .s3
        .get_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .set_range((head.content_length().unwrap_or_default() > 0).then(|| "bytes=0-0".into()))
        .send()
        .await
        .map_err(|_| AppError::Storage("Could not read source object data.".into()))?
        .body;
    if let Some(chunk) = body.next().await {
        chunk.map_err(|_| AppError::Storage("Could not finish the sample read.".into()))?;
    }
    Ok(())
}

async fn verify_probe_object(target: &Session, bucket: &str) -> Result<()> {
    let object = ObjectId {
        bucket: bucket.into(),
        key: PROBE_KEY.into(),
        version_id: None,
    };
    let head = objects::head(&target.s3, &object)
        .await
        .map_err(|_| AppError::Storage("Could not read the temporary object.".into()))?;
    let tags = vec![Tag {
        key: " migration ".into(),
        value: " exact ".into(),
    }];
    ObjectService::change(&target.s3, &object, ObjectChange::SetTags { tags })
        .await
        .map_err(|_| AppError::Storage("Could not write tags to the temporary object.".into()))?;
    if exact_tags(&target.s3, &object).await? != [(" migration ".into(), " exact ".into())] {
        return Err(AppError::Storage(
            "The destination did not preserve exact tags.".into(),
        ));
    }
    let mut body = target
        .s3
        .get_object()
        .bucket(bucket)
        .key(PROBE_KEY)
        .set_range((head.content_length().unwrap_or_default() > 0).then(|| "bytes=0-0".into()))
        .send()
        .await
        .map_err(|_| AppError::Storage("Could not read temporary object data.".into()))?
        .body;
    if let Some(chunk) = body.next().await {
        chunk
            .map_err(|_| AppError::Storage("Could not finish the temporary object read.".into()))?;
    }
    Ok(())
}

async fn test_multipart(
    repository: &MigrationRepository,
    record: &mut Migration,
    target: &Session,
    bucket: &str,
    index: usize,
) -> Result<()> {
    let client = write_client(&target.s3);
    let output = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(MULTIPART_KEY)
        .send()
        .await
        .map_err(|_| AppError::Storage("Could not begin a temporary multipart upload.".into()))?;
    let id = output
        .upload_id()
        .ok_or_else(|| AppError::Storage("The multipart probe omitted its upload ID.".into()))?;
    if let Effect::Probe { upload, .. } = &mut record.private.effects[index] {
        *upload = Some(id.into());
    }
    repository.save(record)?;
    let part = client
        .upload_part()
        .bucket(bucket)
        .key(MULTIPART_KEY)
        .upload_id(id)
        .part_number(1)
        .body(ByteStream::from_static(PROBE_BODY))
        .send()
        .await
        .map_err(|_| AppError::Storage("Could not write the temporary multipart part.".into()))?;
    let part = aws_sdk_s3::types::CompletedPart::builder()
        .part_number(1)
        .set_e_tag(part.e_tag)
        .build();
    let upload = aws_sdk_s3::types::CompletedMultipartUpload::builder()
        .parts(part)
        .build();
    client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(MULTIPART_KEY)
        .upload_id(id)
        .multipart_upload(upload)
        .send()
        .await
        .map_err(|_| AppError::Storage("Could not complete the multipart probe.".into()))?;
    if let Effect::Probe { upload, .. } = &mut record.private.effects[index] {
        *upload = None;
    }
    repository.save(record)
}

pub async fn require_absent(client: &Client, bucket: &str) -> Result<()> {
    match client.head_bucket().bucket(bucket).send().await {
        Ok(_) => Err(AppError::Invalid(
            "The destination already exists, even if it is empty.".into(),
        )),
        Err(error)
            if error
                .raw_response()
                .is_some_and(|response| response.status().as_u16() == 404) =>
        {
            Ok(())
        }
        Err(_) => Err(AppError::Storage(
            "The destination name could not be confirmed available.".into(),
        )),
    }
}

pub async fn create_new_bucket(session: &Session, bucket: &str) -> Result<()> {
    require_absent(&session.s3, bucket).await?;
    let client = write_client(&session.s3);
    let mut request = client.create_bucket().bucket(bucket);
    if session.profile.provider == Provider::Aws && session.profile.region != "us-east-1" {
        use aws_sdk_s3::types::{BucketLocationConstraint, CreateBucketConfiguration};
        request = request.create_bucket_configuration(
            CreateBucketConfiguration::builder()
                .location_constraint(BucketLocationConstraint::from(
                    session.profile.region.as_str(),
                ))
                .build(),
        );
    }
    request.send().await.map_err(|error| {
        match error
            .as_service_error()
            .and_then(ProvideErrorMetadata::code)
        {
            Some("BucketAlreadyOwnedByYou" | "BucketAlreadyExists") => {
                AppError::Invalid("The destination already exists.".into())
            }
            _ => match transfer_error(error, true, true) {
                error @ AppError::AmbiguousWrite(_) => error,
                _ => AppError::Storage("The destination bucket could not be created.".into()),
            },
        }
    })?;
    Ok(())
}

pub async fn storage_copy(
    client: &Client,
    source: &ObjectId,
    target: &str,
    key: &str,
) -> Result<Option<String>> {
    let output = write_client(client)
        .copy_object()
        .bucket(target)
        .key(key)
        .copy_source(objects::copy_source(source))
        .send()
        .await
        .map_err(|error| match transfer_error(error, true, true) {
            error @ AppError::AmbiguousWrite(_) => error,
            _ => AppError::Storage(
                "Storage-side copy failed. Verify the source permissions and object size.".into(),
            ),
        })?;
    Ok(output.version_id)
}

pub async fn exact_tags(client: &Client, object: &ObjectId) -> Result<Vec<(String, String)>> {
    let tags = ObjectService::tags(client, object)
        .await
        .map_err(|_| AppError::Storage("The provider did not return valid object tags.".into()))?;
    let mut tags: Vec<_> = tags.into_iter().map(|tag| (tag.key, tag.value)).collect();
    tags.sort_unstable();
    if tags.iter().any(|(key, _)| key.is_empty())
        || tags.windows(2).any(|pair| pair[0].0 == pair[1].0)
    {
        return Err(AppError::Storage(
            "The provider returned invalid or duplicate tag keys.".into(),
        ));
    }
    Ok(tags)
}

pub async fn policy(client: &Client, bucket: &str) -> Result<Option<Value>> {
    match client.get_bucket_policy().bucket(bucket).send().await {
        Ok(output) => {
            let document = output.policy().ok_or_else(|| {
                AppError::Storage("The provider omitted the bucket policy.".into())
            })?;
            let policy: Value = serde_json::from_str(document).map_err(|_| {
                AppError::Storage("The provider returned an invalid bucket policy.".into())
            })?;
            if !policy.is_object() {
                return Err(AppError::Storage(
                    "The provider returned an invalid bucket policy.".into(),
                ));
            }
            Ok(Some(policy))
        }
        Err(error)
            if error
                .as_service_error()
                .and_then(ProvideErrorMetadata::code)
                == Some("NoSuchBucketPolicy") =>
        {
            Ok(None)
        }
        Err(_) => Err(AppError::Storage(
            "The bucket policy could not be read.".into(),
        )),
    }
}

pub struct PolicyAccess<'a> {
    pub delete_only: bool,
    pub deny_writes: bool,
    pub writer: Option<&'a str>,
    pub read_grant: Option<&'a str>,
}

pub async fn protect(
    repository: &MigrationRepository,
    record: &mut Migration,
    session: &Session,
    side: Side,
    bucket: &str,
    access: PolicyAccess<'_>,
) -> Result<()> {
    let current = policy(&session.s3, bucket).await?;
    let mut receipts = record
        .private
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Policy {
                side: bound,
                bucket: name,
                original,
                protection,
            } if *bound == side && name == bucket => Some((original, protection)),
            _ => None,
        });
    let first = receipts.next();
    let latest = receipts.last().or(first);
    let earliest = first.map_or_else(|| current.clone(), |(original, _)| original.clone());
    let protection = build_protection(earliest, record.id, session, bucket, access)?;
    if let Some((original, last)) = latest {
        if current.as_ref() != original.as_ref() && current.as_ref() != Some(last) {
            return Err(AppError::Invalid(
                "A recorded migration policy changed outside StoreForge.".into(),
            ));
        }
        if *last == protection {
            if current.as_ref() != Some(last) {
                write_policy(&session.s3, bucket, Some(last)).await?;
            }
            return verify_policy(session, bucket, last).await;
        }
        if current.as_ref() != Some(last) {
            return Err(AppError::Invalid(
                "Restore interrupted policy changes before switching protections.".into(),
            ));
        }
    }
    record.private.effects.push(Effect::Policy {
        side,
        bucket: bucket.into(),
        original: current,
        protection: protection.clone(),
    });
    repository.save(record)?;
    write_policy(&session.s3, bucket, Some(&protection)).await?;
    verify_policy(session, bucket, &protection).await
}

pub async fn verify_policy(session: &Session, bucket: &str, expected: &Value) -> Result<()> {
    if policy(&session.s3, bucket).await?.as_ref() != Some(expected) {
        return Err(AppError::Storage(
            "The provider did not verify the recorded protection.".into(),
        ));
    }
    Ok(())
}

pub(super) fn build_protection(
    original: Option<Value>,
    id: Uuid,
    session: &Session,
    bucket: &str,
    access: PolicyAccess<'_>,
) -> Result<Value> {
    let mut document = original.unwrap_or_else(|| json!({ "Version": "2012-10-17" }));
    let mut statements = match document.get("Statement") {
        Some(Value::Array(value)) => value.clone(),
        Some(Value::Object(_)) => vec![document["Statement"].clone()],
        None => Vec::new(),
        _ => {
            return Err(AppError::Invalid(
                "The existing policy statements could not be preserved.".into(),
            ));
        }
    };
    let sid = format!("StoreforgeMigration{}", id.simple());
    if statements.iter().any(|statement| {
        statement["Sid"]
            .as_str()
            .is_some_and(|value| value.starts_with("StoreforgeMigration"))
    }) {
        return Err(AppError::Invalid(
            "The bucket already has a migration protection.".into(),
        ));
    }
    let partition = sts::partition(&session.profile.region);
    let resource = format!("arn:{partition}:s3:::{bucket}/*");
    let mut deny = json!({
        "Sid": sid, "Effect": "Deny",
        "Action": ["s3:PutObject", "s3:DeleteObject", "s3:DeleteObjectVersion", "s3:PutObjectTagging",
                   "s3:DeleteObjectTagging", "s3:PutObjectVersionTagging", "s3:DeleteObjectVersionTagging",
                   "s3:AbortMultipartUpload", "s3:PutObjectAcl", "s3:PutObjectVersionAcl"],
        "Resource": resource,
    });
    if let Some(principal) = access.writer {
        if session.profile.provider == Provider::Ceph {
            deny["NotPrincipal"] = json!({ "AWS": principal });
        } else {
            deny["Principal"] = json!("*");
            deny["Condition"] = json!({ "ArnNotEquals": { "aws:PrincipalArn": principal } });
        }
        if access.delete_only {
            deny["Action"] = json!(["s3:DeleteObject", "s3:DeleteObjectVersion"]);
            statements.push(json!({
                "Sid": format!("{sid}NoOtherWrites"), "Effect": "Deny", "Principal": "*",
                "Action": ["s3:PutObject", "s3:PutObjectTagging", "s3:DeleteObjectTagging", "s3:PutObjectVersionTagging",
                           "s3:DeleteObjectVersionTagging", "s3:PutObjectAcl", "s3:PutObjectVersionAcl"],
                "Resource": resource,
            }));
        }
    }
    if access.writer.is_none() {
        deny["Principal"] = json!("*");
    }
    if access.deny_writes {
        statements.push(deny);
    }
    if let Some(principal) = access.read_grant {
        statements.push(json!({
            "Sid": format!("{sid}Read"), "Effect": "Allow", "Principal": { "AWS": principal },
            "Action": ["s3:GetObject", "s3:GetObjectVersion"], "Resource": resource,
        }));
    }
    document["Statement"] = Value::Array(statements);
    Ok(document)
}

pub async fn restore_policy(
    client: &Client,
    bucket: &str,
    original: Option<&Value>,
    protection: &Value,
) -> Result<()> {
    let current = policy(client, bucket).await?;
    if current.as_ref() == original {
        return Ok(());
    }
    if current.as_ref() != Some(protection) {
        return Err(AppError::Invalid(
            "The bucket policy changed outside the migration. Review it before restoring access."
                .into(),
        ));
    }
    write_policy(client, bucket, original).await?;
    if policy(client, bucket).await?.as_ref() != original {
        return Err(AppError::Storage(
            "The provider did not verify the restored bucket policy.".into(),
        ));
    }
    Ok(())
}

async fn write_policy(client: &Client, bucket: &str, document: Option<&Value>) -> Result<()> {
    let client = write_client(client);
    if let Some(document) = document {
        return client
            .put_bucket_policy()
            .bucket(bucket)
            .policy(document.to_string())
            .send()
            .await
            .map(|_| ())
            .map_err(|_| {
                AppError::Storage("The provider declined the bucket policy write.".into())
            });
    }
    client
        .delete_bucket_policy()
        .bucket(bucket)
        .send()
        .await
        .map(|_| ())
        .map_err(|_| {
            AppError::Storage("The provider declined the removal of temporary protection.".into())
        })
}

pub async fn restore_effect(effect: &Effect, source: &Session, target: &Session) -> Result<()> {
    match effect {
        Effect::Policy {
            side,
            bucket,
            original,
            protection,
        } => {
            let session = if *side == Side::Source {
                source
            } else {
                target
            };
            restore_policy(&session.s3, bucket, original.as_ref(), protection).await
        }
        Effect::Probe {
            bucket,
            created,
            upload,
        } => delete_probe(target, bucket, *created, upload.as_deref()).await,
    }
}

async fn delete_probe(
    target: &Session,
    bucket: &str,
    created: bool,
    upload: Option<&str>,
) -> Result<()> {
    if require_absent(&target.s3, bucket).await.is_ok() {
        return Ok(());
    }
    if !created {
        return Err(AppError::Storage(format!(
            "Creation of temporary bucket {bucket} was unconfirmed. Inspect and remove it before retrying recovery."
        )));
    }
    let client = write_client(&target.s3);
    if let Some(id) = upload {
        let result = client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(MULTIPART_KEY)
            .upload_id(id)
            .send()
            .await;
        if let Err(error) = result
            && error
                .as_service_error()
                .and_then(ProvideErrorMetadata::code)
                != Some("NoSuchUpload")
        {
            return Err(AppError::Storage(
                "Could not abort the temporary multipart upload.".into(),
            ));
        }
    }
    let location = Location {
        connection: target.profile.id,
        bucket: bucket.into(),
        prefix: String::new(),
    };
    let mut versions = Vec::new();
    inventory::visit_versions(&target.s3, &location, |page| versions.extend(page))
        .await
        .map_err(|_| {
            AppError::Storage("Could not list temporary object versions for cleanup.".into())
        })?;
    if versions
        .iter()
        .any(|version| !matches!(version.key.as_str(), PROBE_KEY | MULTIPART_KEY))
    {
        return Err(AppError::Storage("The temporary bucket contains an unexpected object. Inspect it before retrying recovery.".into()));
    }
    for version in versions {
        client
            .delete_object()
            .bucket(bucket)
            .key(version.key)
            .version_id(version.version_id)
            .send()
            .await
            .map_err(|_| {
                AppError::Storage("Could not remove a temporary object version.".into())
            })?;
    }
    for row in inventory::objects(&target.s3, &location)
        .await
        .map_err(|_| {
            AppError::Storage("Could not list current temporary objects for cleanup.".into())
        })?
    {
        if !matches!(row.key.as_str(), PROBE_KEY | MULTIPART_KEY) {
            return Err(AppError::Storage("The temporary bucket contains an unexpected object. Inspect it before retrying recovery.".into()));
        }
        client
            .delete_object()
            .bucket(bucket)
            .key(row.key)
            .send()
            .await
            .map_err(|_| AppError::Storage("Could not remove a temporary object.".into()))?;
    }
    BucketService::delete(target, bucket)
        .await
        .map_err(|_| AppError::Storage(format!("Could not remove temporary bucket {bucket}.")))
}
