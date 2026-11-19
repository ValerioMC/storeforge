use super::*;
use crate::{
    connection::{
        model::{Connection, IdentityKind, Secret, Workspace},
        service::{describe_context, open_session},
    },
    object::service::tests::{object_drawer_client, object_drawer_mock},
};

type Response = (&'static str, String, Vec<u8>);

fn session(endpoint: &str) -> Session {
    let profile: Connection = serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(), "revision": Uuid::new_v4(), "name": "Fixture",
        "provider": "ceph", "endpoint": endpoint, "region": "us-east-1",
        "pathStyle": true, "knownBuckets": [],
    }))
    .unwrap();
    let context = describe_context(&profile, Workspace::Manager, IdentityKind::S3).unwrap();
    let mut session = open_session(
        profile,
        context,
        Secret {
            access_key: "fixture-key".into(),
            secret_key: "fixture-secret".into(),
            session_token: None,
        },
    )
    .unwrap();
    session.s3 = object_drawer_client(endpoint);
    session
}

fn response(status: &'static str, body: &str) -> Response {
    (
        status,
        format!("Content-Length: {}\r\n", body.len()),
        body.as_bytes().to_vec(),
    )
}

fn listing(rows: &[(&str, &str, usize)]) -> Response {
    let xml: String = rows.iter().map(|(key, etag, size)| format!(
        "<Contents><Key>{key}</Key><ETag>&quot;{etag}&quot;</ETag><Size>{size}</Size><LastModified>2025-01-01T00:00:00Z</LastModified></Contents>"
    )).collect();
    response(
        "200 OK",
        &format!("<ListBucketResult><IsTruncated>false</IsTruncated>{xml}</ListBucketResult>"),
    )
}

fn head(etag: &str, size: usize) -> Response {
    (
        "200 OK",
        format!(
            "Content-Length: {size}\r\nETag: \"{etag}\"\r\nLast-Modified: Wed, 01 Jan 2025 00:00:00 GMT\r\n"
        ),
        Vec::new(),
    )
}

fn path(request: &str, method: &str) -> String {
    let mut parts = request.lines().next().unwrap().split_whitespace();
    assert_eq!(parts.next(), Some(method));
    parts.next().unwrap().into()
}

#[tokio::test]
async fn keeps_multipart_etags_inconclusive_and_copies_only_the_reviewed_exact_key() {
    let (source_endpoint, source_requests) = object_drawer_mock(vec![
        listing(&[(" report  ", "put-3", 3), ("report", "left-2", 4)]),
        head("put-3", 3),
        (
            "200 OK",
            "Content-Length: 3\r\nETag: \"put-3\"\r\n".into(),
            b"abc".to_vec(),
        ),
    ]);
    let (target_endpoint, target_requests) = object_drawer_mock(vec![
        listing(&[("report", "right-3", 4), ("report ", "other-3", 3)]),
        response("404 Not Found", ""),
        response("404 Not Found", ""),
        (
            "200 OK",
            "Content-Length: 0\r\nETag: \"copied\"\r\n".into(),
            Vec::new(),
        ),
        head("copied", 3),
    ]);
    let source = session(&source_endpoint);
    let target = session(&target_endpoint);
    let comparison = compare(
        &source,
        &target,
        ComparisonRequest {
            pairs: vec![BucketPair {
                source: "source".into(),
                target: "target".into(),
            }],
            content: ContentMode::Etag,
            modified_before: None,
            configuration: vec![],
        },
    )
    .await
    .unwrap();
    let report = &comparison.reports[0];
    let contents = report.contents.as_ref().unwrap();
    assert_eq!(
        (
            contents.source_only,
            contents.target_only,
            contents.inconclusive
        ),
        (1, 1, 1)
    );
    assert!(
        contents
            .details
            .iter()
            .any(|row| row.key == "report" && row.evidence == Evidence::Inconclusive)
    );
    let row = contents
        .details
        .iter()
        .find(|row| row.key == " report  ")
        .unwrap();
    apply(
        &source,
        &target,
        comparison.scope.clone(),
        report.pair.clone(),
        Remediation::Object {
            key: row.key.clone(),
            source: row.source.clone(),
            target: row.target.clone(),
        },
    )
    .await
    .unwrap();
    let source = source_requests.join().unwrap();
    let target = target_requests.join().unwrap();
    assert_eq!(path(&source[1], "HEAD"), "/source/%20report%20%20");
    assert_eq!(
        path(&source[2], "GET").split('?').next(),
        Some("/source/%20report%20%20")
    );
    assert_eq!(
        path(&target[3], "PUT").split('?').next(),
        Some("/target/%20report%20%20")
    );
    assert!(target[3].to_ascii_lowercase().contains("if-none-match: *"));
    assert_eq!(path(&target[4], "HEAD"), "/target/%20report%20%20");
}

#[tokio::test]
async fn compares_policies_and_applies_only_the_reviewed_section_with_readback() {
    let source_policy = r#"{"Statement":[{"Effect":"Deny"}]}"#;
    let target_policy = r#"{"Statement":[{"Effect":"Allow"}]}"#;
    let (source_endpoint, source_requests) = object_drawer_mock(vec![
        response("200 OK", source_policy),
        response("200 OK", source_policy),
    ]);
    let (target_endpoint, target_requests) = object_drawer_mock(vec![
        response("200 OK", target_policy),
        response("200 OK", target_policy),
        response("204 No Content", ""),
        response("200 OK", source_policy),
    ]);
    let source = session(&source_endpoint);
    let target = session(&target_endpoint);
    let comparison = compare(
        &source,
        &target,
        ComparisonRequest {
            pairs: vec![BucketPair {
                source: "source".into(),
                target: "target".into(),
            }],
            content: ContentMode::None,
            modified_before: None,
            configuration: vec![Configuration::Policy],
        },
    )
    .await
    .unwrap();
    let report = &comparison.reports[0];
    let row = &report.configuration[0];
    assert!(row.different && row.can_apply);
    apply(
        &source,
        &target,
        comparison.scope.clone(),
        report.pair.clone(),
        Remediation::Configuration {
            configuration: row.configuration,
            source: row.source.clone(),
            target: row.target.clone(),
        },
    )
    .await
    .unwrap();
    let source = source_requests.join().unwrap();
    let target = target_requests.join().unwrap();
    assert!(path(&source[1], "GET").ends_with("?policy"));
    assert!(path(&target[2], "PUT").ends_with("?policy"));
    assert!(path(&target[3], "GET").ends_with("?policy"));
}

#[test]
fn excludes_composite_checksums_and_counts_cutoff_keys_separately() {
    let digest = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    for (kind, expected) in [
        (ChecksumType::FullObject, Some(digest.into())),
        (ChecksumType::Composite, None),
    ] {
        let head = HeadObjectOutput::builder()
            .checksum_sha256(digest)
            .checksum_type(kind)
            .build();
        assert_eq!(full_object_digest(&head), expected);
    }
    let row = ObjectRow {
        key: "exact".into(),
        folder: false,
        size: Some(1),
        modified: Some("2025-01-01T00:00:00Z".into()),
        storage_class: None,
        etag: None,
    };
    let cutoff = inventory::parse_modified("2024-12-31T23:59:59Z");
    assert!(matches!(
        cutoff_status(Some(&row), None, cutoff),
        CutoffStatus::After
    ));
}
