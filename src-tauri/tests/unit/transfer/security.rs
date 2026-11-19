use super::*;
use crate::{
    object::{
        model::ObjectId,
        service::{
            ObjectService,
            tests::{object_drawer_client, object_drawer_mock},
        },
    },
    transfer::files::{self, UploadOptions},
};

const CUSTOMER_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

fn fixture_object() -> ObjectId {
    ObjectId {
        bucket: "fixture".into(),
        key: " literal.txt ".into(),
        version_id: None,
    }
}

fn customer_access() -> OperationAccess {
    OperationAccess::parse(
        AccessInput {
            customer_key: Some(CUSTOMER_KEY.into()),
            ..AccessInput::default()
        },
        false,
    )
    .unwrap()
}

#[tokio::test]
async fn object_transfer_policy_uses_customer_headers_for_preview_download_and_upload() {
    let (endpoint, worker) = object_drawer_mock(vec![
        (
            "200 OK",
            "Content-Length: 3\r\nContent-Type: text/plain\r\nETag: \"fixture\"\r\n".into(),
            vec![],
        ),
        ("200 OK", "Content-Length: 3\r\n".into(), b"abc".to_vec()),
        ("200 OK", "Content-Length: 3\r\n".into(), b"abc".to_vec()),
        (
            "200 OK",
            "Content-Length: 0\r\nETag: \"uploaded\"\r\n".into(),
            vec![],
        ),
    ]);
    let client = object_drawer_client(&endpoint);
    let object = fixture_object();
    let access = customer_access();
    let preview = ObjectService::preview(&client, &object, access.read_access())
        .await
        .unwrap();
    assert_eq!(STANDARD.decode(preview.data).unwrap(), b"abc");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("download.txt");
    let size = files::download(
        &client,
        &object,
        &path,
        false,
        access.read_access(),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(size, 3);
    assert_eq!(std::fs::read(&path).unwrap(), b"abc");
    files::upload(
        &client,
        &object.bucket,
        &object.key,
        &path,
        UploadOptions {
            access: access.upload_access(),
            content_type: "text/plain".into(),
            ..UploadOptions::default()
        },
    )
    .await
    .unwrap();

    let requests = worker.join().unwrap();
    let checksum = access.customer.as_ref().unwrap().md5();
    for request in &requests {
        assert!(request.contains("/fixture/%20literal.txt%20"));
        assert!(request.contains("x-amz-server-side-encryption-customer-algorithm: AES256"));
        assert!(request.contains(&format!(
            "x-amz-server-side-encryption-customer-key: {CUSTOMER_KEY}"
        )));
        assert!(request.contains(&format!(
            "x-amz-server-side-encryption-customer-key-md5: {checksum}"
        )));
    }
    assert!(requests[3].contains("if-none-match: *"));
}

#[tokio::test]
async fn object_transfer_policy_refuses_a_customer_key_signed_url() {
    let (endpoint, worker) = object_drawer_mock(vec![(
        "400 Bad Request",
        "Content-Length: 0\r\n".into(),
        vec![],
    )]);
    let object = fixture_object();
    let failure = ObjectService::share_url(&object_drawer_client(&endpoint), &object, 5)
        .await
        .unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("shared without a customer key")
    );
    assert!(worker.join().unwrap()[0].starts_with("HEAD "));
}

#[tokio::test]
async fn object_transfer_policy_classifies_ambiguous_write_and_disables_sdk_retries() {
    let (endpoint, worker) = object_drawer_mock(vec![(
        "503 Service Unavailable",
        "Content-Length: 0\r\n".into(),
        vec![],
    )]);
    let mut config = object_drawer_client(&endpoint).config().to_builder();
    config = config.retry_config(RetryConfig::standard().with_max_attempts(4));
    let client = Client::from_conf(config.build());
    let object = fixture_object();
    let access = customer_access();
    let source = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(source.path(), b"fixture").unwrap();
    let result = files::upload(
        &client,
        &object.bucket,
        &object.key,
        source.path(),
        UploadOptions {
            access: access.upload_access(),
            overwrite: true,
            ..UploadOptions::default()
        },
    )
    .await;
    assert!(matches!(result, Err(AppError::AmbiguousWrite(_))));
    assert_eq!(
        write_client(&client)
            .config()
            .retry_config()
            .unwrap()
            .max_attempts(),
        1
    );
    assert_eq!(worker.join().unwrap().len(), 1);
}

#[tokio::test]
async fn object_transfer_policy_redacts_provider_errors_that_echo_a_customer_key() {
    let reply =
        format!("<Error><Code>InvalidArgument</Code><Message>{CUSTOMER_KEY}</Message></Error>");
    let (endpoint, worker) = object_drawer_mock(vec![(
        "400 Bad Request",
        format!("Content-Length: {}\r\n", reply.len()),
        reply.into_bytes(),
    )]);
    let access = customer_access();
    let client = object_drawer_client(&endpoint);
    let result = files::download(
        &client,
        &fixture_object(),
        &tempfile::tempdir().unwrap().path().join("out"),
        false,
        access.read_access(),
        &|_| Ok(()),
    )
    .await;
    let failure = result.unwrap_err().to_string();
    assert!(failure.contains("rejected this encrypted"));
    assert!(!failure.contains(CUSTOMER_KEY));
    assert_eq!(worker.join().unwrap().len(), 1);
}

#[tokio::test]
async fn object_transfer_policy_uploads_with_each_server_encryption_choice() {
    let (endpoint, worker) = object_drawer_mock(vec![
        (
            "200 OK",
            "Content-Length: 0\r\nETag: \"uploaded\"\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            "Content-Length: 0\r\nETag: \"uploaded\"\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            "Content-Length: 0\r\nETag: \"uploaded\"\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            "Content-Length: 0\r\nETag: \"uploaded\"\r\n".into(),
            vec![],
        ),
    ]);
    let client = object_drawer_client(&endpoint);
    let object = fixture_object();
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"fixture").unwrap();
    for encryption in [
        None,
        Some(UploadEncryption::S3),
        Some(UploadEncryption::Kms {
            key_id: Some("fixture-key-id".into()),
        }),
        Some(UploadEncryption::DualLayerKms {
            key_id: Some("fixture-key-id".into()),
        }),
    ] {
        files::upload(
            &client,
            &object.bucket,
            &object.key,
            file.path(),
            UploadOptions {
                access: UploadAccess {
                    encryption: encryption.as_ref(),
                    ..UploadAccess::default()
                },
                overwrite: true,
                ..UploadOptions::default()
            },
        )
        .await
        .unwrap();
    }
    let requests = worker.join().unwrap();
    assert!(!requests[0].contains("x-amz-server-side-encryption:"));
    assert!(requests[1].contains("x-amz-server-side-encryption: AES256"));
    assert!(requests[2].contains("x-amz-server-side-encryption: aws:kms\r\n"));
    assert!(requests[3].contains("x-amz-server-side-encryption: aws:kms:dsse\r\n"));
    assert!(requests[2..].iter().all(|request| {
        request.contains("x-amz-server-side-encryption-aws-kms-key-id: fixture-key-id")
    }));
}
