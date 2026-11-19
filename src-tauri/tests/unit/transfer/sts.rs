use super::*;
use crate::object::service::tests::{object_drawer_client, object_drawer_mock};
use crate::transfer::{files, security::ReadAccess};

fn fixture_xml(expiration: &str) -> Zeroizing<String> {
    Zeroizing::new(format!(
        "<AssumeRoleResponse><AssumeRoleResult><Credentials><AccessKeyId>temporary-fixture</AccessKeyId><SecretAccessKey>temporary-fixture-secret</SecretAccessKey><SessionToken>temporary-token</SessionToken><Expiration>{expiration}</Expiration></Credentials></AssumeRoleResult></AssumeRoleResponse>"
    ))
}

#[test]
fn object_transfer_policy_scopes_the_role_to_literal_resources() {
    let object = ObjectId {
        bucket: "archive".into(),
        key: "logs/ a ".into(),
        version_id: Some("v 1".into()),
    };
    let policy = serialize_policy(
        "aws",
        RoleScope::Read(std::slice::from_ref(&object)),
        None,
        None,
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&policy).unwrap();
    assert_eq!(parsed["Statement"][0]["Action"][0], "s3:GetObjectVersion");
    assert_eq!(
        parsed["Statement"][0]["Resource"][0],
        "arn:aws:s3:::archive/logs/ a "
    );
    assert_eq!(
        parsed["Statement"][0]["Condition"]["StringEquals"]["s3:VersionId"][0],
        "v 1"
    );
    assert!(
        serialize_policy(
            "aws",
            RoleScope::Read(&[ObjectId {
                key: "logs/*".into(),
                ..object
            }]),
            None,
            None
        )
        .is_err()
    );
}

#[test]
fn object_transfer_policy_requests_renewal_when_sts_returns_an_expired_grant() {
    let error = parse_credentials(fixture_xml("2000-11-19T00:00:00Z")).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Renew the temporary credentials")
    );
    assert!(!error.to_string().contains("temporary-fixture"));
}

#[tokio::test]
async fn object_transfer_policy_signs_a_download_with_issued_temporary_credentials() {
    let (endpoint, worker) = object_drawer_mock(vec![(
        "200 OK",
        "Content-Length: 3\r\n".into(),
        b"sts".to_vec(),
    )]);
    let credentials = parse_credentials(fixture_xml("2099-11-19T00:00:00Z")).unwrap();
    let config = object_drawer_client(&endpoint)
        .config()
        .to_builder()
        .credentials_provider(credentials)
        .build();
    let object = ObjectId {
        bucket: "archive".into(),
        key: "logs/ a ".into(),
        version_id: Some("v 1".into()),
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("download");
    files::download(
        &aws_sdk_s3::Client::from_conf(config),
        &object,
        &path,
        false,
        ReadAccess {
            sensitive: true,
            ..ReadAccess::default()
        },
        &|_| Ok(()),
    )
    .await
    .unwrap();
    let request = worker.join().unwrap().remove(0);
    assert!(request.contains("x-amz-security-token: temporary-token"));
    assert!(request.contains("versionId=v%201"));
    assert_eq!(std::fs::read(path).unwrap(), b"sts");
}

#[test]
fn object_transfer_policy_restricts_kms_permission_to_s3_and_the_object_context() {
    let keys = vec!["new/ report.csv ".into()];
    let policy = serialize_policy(
        "aws",
        RoleScope::Write {
            bucket: "archive",
            keys: &keys,
        },
        Some(&UploadEncryption::Kms {
            key_id: Some("arn:aws:kms:us-east-1:123456789012:key/test-id".into()),
        }),
        Some("s3.us-east-1.amazonaws.com"),
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&policy).unwrap();
    let kms = &parsed["Statement"][1];
    assert_eq!(
        kms["Resource"][0],
        "arn:aws:kms:us-east-1:123456789012:key/test-id"
    );
    assert_eq!(
        kms["Condition"]["StringEquals"]["kms:ViaService"][0],
        "s3.us-east-1.amazonaws.com"
    );
    assert_eq!(
        kms["Condition"]["StringEquals"]["kms:EncryptionContext:aws:s3:arn"][1],
        "arn:aws:s3:::archive/new/ report.csv "
    );
}
