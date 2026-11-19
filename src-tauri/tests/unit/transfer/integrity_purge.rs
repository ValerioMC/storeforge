use super::*;
use crate::{
    connection::{
        model::{Connection, IdentityKind, Secret, Workspace},
        service::{describe_context, open_session},
    },
    object::service::tests::{object_drawer_client, object_drawer_mock},
};

type Response = (&'static str, String, Vec<u8>);

fn fixture(endpoint: &str) -> Session {
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

fn selection(session: &Session, buckets: &[&str]) -> BucketSelection {
    BucketSelection {
        connection: session.profile.id,
        parallelism: 1,
        targets: buckets
            .iter()
            .map(|bucket| BucketScope {
                bucket: (*bucket).into(),
                prefix: String::new(),
            })
            .collect(),
    }
}

fn response(status: &'static str, body: &str) -> Response {
    (
        status,
        format!("Content-Length: {}\r\n", body.len()),
        body.as_bytes().to_vec(),
    )
}

fn listing(entries: &str) -> Response {
    response(
        "200 OK",
        &format!(
            "<ListVersionsResult><IsTruncated>false</IsTruncated>{entries}</ListVersionsResult>"
        ),
    )
}

fn version(key: &str, id: &str, latest: bool, date: &str) -> String {
    format!(
        "<Version><Key>{key}</Key><VersionId>{id}</VersionId><IsLatest>{latest}</IsLatest><LastModified>{date}</LastModified><Size>6</Size></Version>"
    )
}

fn marker(key: &str, id: &str) -> String {
    format!(
        "<DeleteMarker><Key>{key}</Key><VersionId>{id}</VersionId><IsLatest>true</IsLatest><LastModified>2025-01-02T00:00:00Z</LastModified></DeleteMarker>"
    )
}

fn denied() -> Response {
    response(
        "403 Forbidden",
        "<Error><Code>AccessDenied</Code><Message>Denied</Message></Error>",
    )
}

#[tokio::test]
async fn integrity_reports_listing_denial_and_reads_bounded_versions_without_markers() {
    let entries = [
        version("current", "current-id", true, "2025-01-02T00:00:00Z"),
        version("archive/ exact ", "older /+", false, "2025-01-03T00:00:00Z"),
        version("excluded", "too-old", false, "2024-12-31T00:00:00Z"),
        marker("hidden", "marker"),
    ]
    .join("");
    let (endpoint, server) = object_drawer_mock(vec![
        denied(),
        listing(&entries),
        (
            "206 Partial Content",
            "Content-Length: 3\r\nContent-Range: bytes 0-2/6\r\n".into(),
            b"abc".to_vec(),
        ),
        response("404 Not Found", "<Error><Code>NoSuchVersion</Code></Error>"),
    ]);
    let session = fixture(&endpoint);
    let request = IntegrityRequest {
        selection: selection(&session, &["denied", "readable"]),
        method: IntegrityMethod::Sample { bytes: 3 },
        since: inventory::parse_modified("2025-01-01T00:00:00Z"),
        include_versions: true,
    };
    let denied = run_integrity(
        &session,
        request.selection.targets[0].clone(),
        &request,
        |_| Ok(true),
    )
    .await
    .unwrap();
    assert_eq!(denied.status, BucketStatus::Failed);
    assert_eq!(denied.failures[0].stage, FailureStage::Listing);

    let result = run_integrity(
        &session,
        request.selection.targets[1].clone(),
        &request,
        |_| Ok(true),
    )
    .await
    .unwrap();
    assert_eq!(
        result.planned,
        BucketCounts {
            current: 1,
            history: 1,
            markers: 0
        }
    );
    assert_eq!(
        result.completed,
        BucketCounts {
            current: 0,
            history: 1,
            markers: 0
        }
    );
    assert_eq!(result.failures[0].stage, FailureStage::Get);
    assert_eq!(result.failures[0].version.as_deref(), Some("current-id"));

    let requests = server.join().unwrap();
    assert!(requests[2].contains("/readable/archive/%20exact%20?"));
    assert!(requests[2].contains("versionId=older%20%2F%2B"));
    assert!(
        requests[2]
            .to_ascii_lowercase()
            .contains("range: bytes=0-2")
    );
    assert!(requests[3].contains("versionId=current-id"));
    assert!(
        requests
            .iter()
            .all(|request| !request.contains("versionId=marker")
                && !request.contains("versionId=too-old"))
    );
}

#[tokio::test]
async fn failed_history_deletion_reports_partial_counts_and_preserves_bucket_and_current_entries() {
    let entries = [
        version("same", "bad-history", false, "2025-01-01T00:00:00Z"),
        version("same", "old-history", false, "2025-01-02T00:00:00Z"),
        version("same", "latest", true, "2025-01-03T00:00:00Z"),
        marker("hidden", "latest-marker"),
    ]
    .join("");
    let (endpoint, server) = object_drawer_mock(vec![
        denied(),
        listing(&entries),
        listing(&entries),
        response("204 No Content", ""),
        denied(),
    ]);
    let session = fixture(&endpoint);
    let request = PurgeRequest {
        selection: selection(&session, &["blocked", "reviewed"]),
        include_versions: true,
        remove_bucket: true,
    };
    let preview = preview_purge(&session, request).await.unwrap();
    assert_eq!(preview.confirmation, "reviewed");
    assert!(preview.validate(&session, "blocked, reviewed").is_err());
    preview.validate(&session, "reviewed").unwrap();
    assert!(preview.buckets[0].counts.is_none());
    assert_eq!(
        preview.buckets[1].counts,
        Some(BucketCounts {
            current: 1,
            history: 2,
            markers: 1
        })
    );
    let blocked = run_purge(&session, &preview.buckets[0], &preview.request, |_| {
        Ok(true)
    })
    .await
    .unwrap();
    assert_eq!(blocked.failures[0].stage, FailureStage::Listing);

    let result = run_purge(&session, &preview.buckets[1], &preview.request, |_| {
        Ok(true)
    })
    .await
    .unwrap();
    assert_eq!(result.status, BucketStatus::Failed);
    assert_eq!(
        result.completed,
        BucketCounts {
            current: 0,
            history: 1,
            markers: 0
        }
    );
    assert_eq!(result.failures[0].stage, FailureStage::Delete);
    assert_eq!(result.failures[0].version.as_deref(), Some("bad-history"));
    assert!(!result.removed);

    let requests = server.join().unwrap();
    assert!(
        requests[3].starts_with("DELETE /reviewed/same?")
            && requests[3].contains("versionId=old-history")
    );
    assert!(
        requests[4].starts_with("DELETE /reviewed/same?")
            && requests[4].contains("versionId=bad-history")
    );
}

#[tokio::test]
async fn purge_deletes_bucket_only_after_all_reviewed_versions_succeed() {
    let entries = [
        version("hidden", "old", false, "2025-01-01T00:00:00Z"),
        marker("hidden", "marker"),
    ]
    .join("");
    let (endpoint, server) = object_drawer_mock(vec![
        listing(&entries),
        listing(&entries),
        response("204 No Content", ""),
        response("204 No Content", ""),
        response("204 No Content", ""),
    ]);
    let session = fixture(&endpoint);
    let preview = preview_purge(
        &session,
        PurgeRequest {
            selection: selection(&session, &["reviewed"]),
            include_versions: true,
            remove_bucket: true,
        },
    )
    .await
    .unwrap();
    let result = run_purge(&session, &preview.buckets[0], &preview.request, |_| {
        Ok(true)
    })
    .await
    .unwrap();
    assert_eq!(result.status, BucketStatus::Completed);
    assert_eq!(
        result.completed,
        BucketCounts {
            current: 0,
            history: 1,
            markers: 1
        }
    );
    assert!(result.removed);
    let requests = server.join().unwrap();
    assert!(requests[2].contains("versionId=old"));
    assert!(requests[3].contains("versionId=marker"));
    assert!(requests[4].starts_with("DELETE /reviewed"));
    assert!(!requests[4].contains("versionId="));
}

#[tokio::test]
async fn full_get_fails_when_the_download_body_is_truncated() {
    let (endpoint, requests) = object_drawer_mock(vec![(
        "200 OK",
        "Content-Length: 6\r\n".into(),
        b"abc".to_vec(),
    )]);
    let session = fixture(&endpoint);
    let item = ScanItem {
        object: ObjectId {
            bucket: "bucket".into(),
            key: "file".into(),
            version_id: None,
        },
        latest: true,
        marker: false,
        size: Some(6),
        etag: None,
        modified: None,
    };
    assert!(
        check_item(&session.s3, &item, IntegrityMethod::Full)
            .await
            .is_err()
    );
    assert!(
        !requests.join().unwrap()[0]
            .to_ascii_lowercase()
            .contains("range:")
    );
}
