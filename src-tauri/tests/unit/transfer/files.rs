use super::*;
use crate::{
    object::service::tests::{object_drawer_client, object_drawer_mock},
    transfer::security::{AccessInput, OperationAccess},
};

fn xml(body: &str) -> (&'static str, String, Vec<u8>) {
    (
        "200 OK",
        format!(
            "Content-Type: application/xml\r\nContent-Length: {}\r\n",
            body.len()
        ),
        body.as_bytes().to_vec(),
    )
}

#[tokio::test]
async fn object_transfer_policy_sends_customer_headers_for_all_multipart_write_stages() {
    let (endpoint, worker) = object_drawer_mock(vec![
        xml(
            "<InitiateMultipartUploadResult><Bucket>fixture</Bucket><Key>literal</Key><UploadId>fixture-id</UploadId></InitiateMultipartUploadResult>",
        ),
        (
            "200 OK",
            "Content-Length: 0\r\nETag: \"part\"\r\n".into(),
            vec![],
        ),
        xml(
            "<CompleteMultipartUploadResult><Bucket>fixture</Bucket><Key>literal</Key><ETag>&quot;complete&quot;</ETag></CompleteMultipartUploadResult>",
        ),
    ]);
    let key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    let access = OperationAccess::parse(
        AccessInput {
            customer_key: Some(key.into()),
            ..AccessInput::default()
        },
        true,
    )
    .unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"multipart fixture").unwrap();
    let client = object_drawer_client(&endpoint);
    let receipt = multipart_upload(
        &client,
        "fixture",
        "literal",
        file.path(),
        17,
        UploadOptions {
            access: access.upload_access(),
            ..UploadOptions::default()
        },
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(receipt.etag.as_deref(), Some("\"complete\""));
    let requests = worker.join().unwrap();
    let header = format!("x-amz-server-side-encryption-customer-key: {key}");
    assert!(requests.iter().all(|request| request.contains(&header)));
    assert!(requests[2].contains("if-none-match: *"));
}
