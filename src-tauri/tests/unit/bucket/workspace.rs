use super::*;
use crate::{
    bucket::{
        model::{BucketQuota, BucketQuotaUpdate, Configuration, ConfigurationChange},
        quota, repository,
    },
    connection::{
        model::{Connection, IdentityKind, Secret, Workspace},
        service::{describe_context, open_session},
    },
    object::service::tests::{object_drawer_client, object_drawer_mock},
};

fn fixture_session(endpoint: &str, identity: IdentityKind) -> Session {
    let id = uuid::Uuid::new_v4();
    let profile: Connection = serde_json::from_value(serde_json::json!({
        "id": id,
        "name": "Fixture",
        "provider": "ceph",
        "endpoint": endpoint,
        "region": "us-east-1",
        "pathStyle": true,
        "knownBuckets": [],
        "settings": { "tenant": "tenant-a" },
        "services": { "cephAdmin": {
            "principalId": uuid::Uuid::new_v4(), "endpoint": endpoint, "enabled": true,
            "managedExternally": false, "capability": "detected"
        } }
    }))
    .unwrap();
    let context = describe_context(&profile, Workspace::Manager, identity).unwrap();
    let credentials = Secret {
        access_key: "fixture-key".into(),
        secret_key: "fixture-secret".into(),
        session_token: None,
    };
    let mut session = open_session(profile, context, credentials).unwrap();
    session.s3 = object_drawer_client(endpoint);
    session
}

fn response(status: &'static str, body: &str) -> (&'static str, String, Vec<u8>) {
    (
        status,
        format!("Content-Length: {}\r\n", body.len()),
        body.as_bytes().to_vec(),
    )
}

fn assert_bucket_request(request: &str, method: &str, query: Option<&str>) {
    let mut line = request.lines().next().unwrap().split_whitespace();
    assert_eq!(line.next(), Some(method));
    let target: http::Uri = line.next().unwrap().parse().unwrap();
    assert_eq!(target.path().trim_end_matches('/'), "/bucket");
    assert_eq!(
        target.query().map(|value| value.trim_end_matches('=')),
        query
    );
}

#[tokio::test]
async fn partial_creation_retries_only_versioning() {
    let (endpoint, requests) = object_drawer_mock(vec![
        response("200 OK", ""),
        response(
            "403 Forbidden",
            "<Error><Code>AccessDenied</Code><Message>Denied</Message></Error>",
        ),
        response("200 OK", "<VersioningConfiguration/>"),
        response("200 OK", ""),
    ]);
    let session = fixture_session(&endpoint, IdentityKind::S3);

    let result = BucketService::create(&session, "bucket", false, Some("zone:fast"), true)
        .await
        .unwrap();
    assert!(result.versioning_error.unwrap().contains("AccessDenied"));
    BucketService::finish_versioning(&session, "bucket", None)
        .await
        .unwrap();

    let requests = requests.join().unwrap();
    assert_bucket_request(&requests[0], "PUT", None);
    assert!(requests[0].to_lowercase().contains("content-length:"));
    assert_bucket_request(&requests[1], "PUT", Some("versioning"));
    assert_bucket_request(&requests[2], "GET", Some("versioning"));
    assert_bucket_request(&requests[3], "PUT", Some("versioning"));
}

#[tokio::test]
async fn object_lock_uses_the_creation_request_and_recovery_recognizes_enabled_versioning() {
    let (endpoint, requests) = object_drawer_mock(vec![
        response("200 OK", ""),
        response(
            "200 OK",
            "<VersioningConfiguration><Status>Enabled</Status></VersioningConfiguration>",
        ),
    ]);
    let session = fixture_session(&endpoint, IdentityKind::S3);
    let result = BucketService::create(&session, "bucket", true, None, true)
        .await
        .unwrap();
    assert!(result.versioning_error.is_none());
    BucketService::finish_versioning(&session, "bucket", None)
        .await
        .unwrap();

    let requests = requests.join().unwrap();
    assert!(
        requests[0]
            .to_lowercase()
            .contains("x-amz-bucket-object-lock-enabled: true")
    );
    assert_bucket_request(&requests[1], "GET", Some("versioning"));
}

#[tokio::test]
async fn configuration_read_distinguishes_an_unconfigured_setting_from_a_missing_bucket() {
    let (endpoint, requests) = object_drawer_mock(vec![
        response(
            "404 Not Found",
            "<Error><Code>NoSuchLifecycleConfiguration</Code></Error>",
        ),
        response("404 Not Found", "<Error><Code>NoSuchBucket</Code></Error>"),
    ]);
    let session = fixture_session(&endpoint, IdentityKind::S3);
    let first = repository::configure(
        &session,
        "bucket",
        Configuration::Lifecycle,
        ConfigurationChange::Read,
    )
    .await;
    let second = repository::configure(
        &session,
        "bucket",
        Configuration::Lifecycle,
        ConfigurationChange::Read,
    )
    .await;
    assert!(matches!(
        first,
        Err(crate::common::error::AppError::Missing(_))
    ));
    assert!(matches!(
        second,
        Err(crate::common::error::AppError::Storage(_))
    ));
    requests.join().unwrap();
}

fn quota_values(max_bytes: &str) -> BucketQuota {
    BucketQuota {
        enabled: true,
        max_bytes: max_bytes.into(),
        max_objects: "27".into(),
    }
}

#[tokio::test]
async fn individual_quota_uses_the_selected_tenant_and_rejects_stale_limits() {
    let existing = r#"{"bucket_quota":{"enabled":true,"max_size":9007199254740993,"max_objects":27,"check_on_raw":true}}"#;
    let (endpoint, requests) = object_drawer_mock(vec![
        response("200 OK", existing),
        response("200 OK", existing),
        response("200 OK", existing),
        response("200 OK", ""),
    ]);
    let session = fixture_session(&endpoint, IdentityKind::CephAdmin);
    let original = quota::read(&session, "bucket").await.unwrap();
    assert_eq!(original.max_bytes, "9007199254740993");
    assert!(
        quota::write(
            &session,
            "bucket",
            BucketQuotaUpdate {
                expected: quota_values("1024"),
                draft: quota_values("2048"),
            }
        )
        .await
        .is_err()
    );
    quota::write(
        &session,
        "bucket",
        BucketQuotaUpdate {
            expected: original,
            draft: quota_values("2048"),
        },
    )
    .await
    .unwrap();
    assert!(quota::read(&session, "tenant-b/bucket").await.is_err());

    let requests = requests.join().unwrap();
    assert!(
        requests[..3]
            .iter()
            .all(|request| request.starts_with("GET /admin/bucket?")
                && request.contains("bucket=tenant-a%2Fbucket"))
    );
    assert!(requests[3].starts_with("PUT /admin/bucket?"));
    assert!(requests[3].contains("bucket=tenant-a%2Fbucket&uid=&quota="));
    assert!(
        requests[3]
            .to_lowercase()
            .contains("content-type: application/json")
    );
}
