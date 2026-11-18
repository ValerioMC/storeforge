use super::*;

#[test]
fn rejects_credentials_in_endpoint_urls() {
    assert!(validate_endpoint("https://key:secret@storage.example.test", Provider::Ceph).is_err());
    assert!(
        validate_endpoint("https://storage.example.test?token=secret", Provider::Ceph).is_err()
    );
}

#[test]
fn preserves_custom_endpoint_prefix() {
    assert_eq!(
        validate_endpoint("https://storage.example.test/s3/", Provider::Ceph).unwrap(),
        "https://storage.example.test/s3",
    );
    assert_eq!(validate_endpoint("", Provider::Aws).unwrap(), "");
}
