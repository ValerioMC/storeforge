use std::{collections::BTreeMap, fs};

use crate::{
    administration::{model::AdminRequest, service::AdministrationService},
    audit::service::AuditService,
    bucket::{
        model::{Configuration, ConfigurationChange},
        repository as buckets,
        service::BucketService,
    },
    connection::{
        model::{Connection, IdentityKind, Provider, Secret, Workspace},
        service::{describe_context, open_session},
    },
    object::{
        model::{Location, ObjectChange, ObjectId, ObjectQuery, Tag},
        repository as objects,
        service::ObjectService,
    },
    transfer::{
        files::{self, UploadOptions},
        inventory,
    },
};

#[tokio::test]
#[ignore = "requires a disposable S3-compatible test endpoint"]
async fn endpoint_object_lifecycle() {
    let endpoint = std::env::var("STOREFORGE_TEST_S3_ENDPOINT").expect("test endpoint");
    let profile = Connection {
        id: uuid::Uuid::new_v4(),
        provider: if std::env::var_os("STOREFORGE_TEST_CEPH").is_some() {
            Provider::Ceph
        } else {
            Provider::Compatible
        },
        name: "Integration".into(),
        endpoint,
        region: std::env::var("STOREFORGE_TEST_S3_REGION").unwrap_or_else(|_| "us-east-1".into()),
        path_style: true,
        known_buckets: Vec::new(),
        endpoint_id: None,
        revision: None,
        settings: Default::default(),
        services: Default::default(),
        s3_capability: Default::default(),
    };
    let credentials = Secret {
        access_key: std::env::var("STOREFORGE_TEST_S3_ACCESS_KEY")
            .unwrap_or_else(|_| "testing".into()),
        secret_key: std::env::var("STOREFORGE_TEST_S3_SECRET_KEY")
            .unwrap_or_else(|_| "testing".into()),
        session_token: None,
    };
    let context = describe_context(&profile, Workspace::Browser, IdentityKind::S3).unwrap();
    let session = open_session(profile, context, credentials).unwrap();
    let bucket = format!("storeforge-test-{}", uuid::Uuid::new_v4().simple());
    BucketService::create(&session, &bucket, false)
        .await
        .unwrap();
    let location = Location {
        connection: session.profile.id,
        bucket: bucket.clone(),
        prefix: "sensors//".into(),
    };

    let document = r#"<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Status>Enabled</Status></VersioningConfiguration>"#;
    buckets::configure(
        &session,
        &bucket,
        Configuration::Versioning,
        ConfigurationChange::Write(document.into()),
    )
    .await
    .unwrap();
    if std::env::var_os("STOREFORGE_TEST_CEPH").is_some() {
        verify_ceph_admin(&session, &bucket).await;
    }
    let versioning = buckets::configure(
        &session,
        &bucket,
        Configuration::Versioning,
        ConfigurationChange::Read,
    )
    .await
    .unwrap();
    assert!(versioning.contains("<Status>Enabled</Status>"));

    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.txt");
    let target = directory.path().join("download.txt");
    let object = ObjectId {
        bucket: bucket.clone(),
        key: "sensors// sample+1.txt ".into(),
        version_id: None,
    };
    fs::write(&source, "test payload").unwrap();
    files::upload(
        &session.s3,
        &bucket,
        &object.key,
        &source,
        UploadOptions {
            content_type: "text/plain".into(),
            overwrite: false,
            ..UploadOptions::default()
        },
    )
    .await
    .unwrap();

    let duplicate = files::upload(
        &session.s3,
        &bucket,
        &object.key,
        &source,
        UploadOptions {
            content_type: "text/plain".into(),
            overwrite: false,
            ..UploadOptions::default()
        },
    )
    .await;
    assert!(
        duplicate.is_err(),
        "destination protection must reject duplicate current keys"
    );
    let page = objects::list(
        &session.s3,
        &ObjectQuery {
            location: location.clone(),
            cursor: None,
            recursive: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(&page.rows[0].key, &object.key);

    let details = ObjectService::details(&session.s3, &object, Default::default())
        .await
        .unwrap();
    assert_eq!(details.size, Some(12));
    let preview = ObjectService::preview(&session.s3, &object, Default::default())
        .await
        .unwrap();
    assert!(!preview.truncated);
    ObjectService::change(
        &session.s3,
        &object,
        ObjectChange::SetTags {
            tags: vec![Tag {
                key: "sensor".into(),
                value: "test".into(),
            }],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        ObjectService::tags(&session.s3, &object).await.unwrap()[0].value,
        "test"
    );
    files::download(
        &session.s3,
        &object,
        &target,
        false,
        Default::default(),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(fs::read(target).unwrap(), b"test payload");
    assert!(
        ObjectService::share_url(&session.s3, &object, 30)
            .await
            .unwrap()
            .contains("X-Amz-Signature=")
    );
    assert_eq!(
        inventory::integrity(&session.s3, &location)
            .await
            .unwrap()
            .failed,
        0
    );

    ObjectService::delete(&session.s3, &object).await.unwrap();
    let versions = objects::versions(&session.s3, &bucket, &object.key, true, None)
        .await
        .unwrap();
    assert_eq!(versions.versions.len(), 2);
    let marker = versions
        .versions
        .iter()
        .find(|version| version.delete_marker)
        .unwrap();
    ObjectService::delete(
        &session.s3,
        &ObjectId {
            version_id: Some(marker.version_id.clone()),
            ..object.clone()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        ObjectService::details(&session.s3, &object, Default::default())
            .await
            .unwrap()
            .size,
        Some(12)
    );

    for version in inventory::all_versions(&session.s3, &location)
        .await
        .unwrap()
    {
        ObjectService::delete(&session.s3, &version).await.unwrap();
    }
    verify_multipart(&session, &location).await;
    BucketService::delete(&session, &bucket).await.unwrap();
}

async fn verify_ceph_admin(session: &crate::connection::service::Session, bucket: &str) {
    let directory = tempfile::tempdir().unwrap();
    let audit = AuditService::open(directory.path().to_owned()).unwrap();
    let response = AdministrationService::new()
        .unwrap()
        .execute(
            session,
            AdminRequest {
                operation: "ceph-buckets-inspect-bucket-statistics".into(),
                values: BTreeMap::from([("bucket".into(), bucket.into())]),
                confirmed: false,
            },
            &audit,
        )
        .await
        .unwrap();
    assert!(response.contains(bucket));
    assert_eq!(audit.list().unwrap()[0].action, "Inspect bucket statistics");
}

async fn verify_multipart(session: &crate::connection::service::Session, location: &Location) {
    let file = tempfile::NamedTempFile::new().unwrap();
    let size = 64 * 1024 * 1024;
    file.as_file().set_len(size).unwrap();
    let object = ObjectId {
        bucket: location.bucket.clone(),
        key: "multipart.bin".into(),
        version_id: None,
    };
    let receipt = files::upload(
        &session.s3,
        &object.bucket,
        &object.key,
        file.path(),
        UploadOptions {
            content_type: "application/octet-stream".into(),
            overwrite: false,
            ..UploadOptions::default()
        },
    )
    .await
    .unwrap();
    let version = ObjectId {
        version_id: receipt.version_id,
        ..object
    };
    let details = objects::head(&session.s3, &version).await.unwrap();
    assert_eq!(details.content_length(), Some(size as i64));
    assert_eq!(
        details.e_tag().map(|value| value.trim_matches('"')),
        receipt.etag.as_deref().map(|value| value.trim_matches('"'))
    );
    ObjectService::delete(&session.s3, &version).await.unwrap();
}
