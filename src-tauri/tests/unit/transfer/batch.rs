use super::*;
use crate::object::model::ObjectVersion;

fn version(key: &str, id: &str, latest: bool, marker: bool, modified: &str) -> ObjectVersion {
    ObjectVersion {
        key: key.into(),
        version_id: id.into(),
        latest,
        delete_marker: marker,
        size: None,
        modified: Some(modified.into()),
        storage_class: None,
    }
}

#[test]
fn browser_batch_cleanup_keeps_latest_and_protects_unreviewed_history() {
    let rows = [
        version(" a ", "v3", true, false, "2026-11-03T00:00:00Z"),
        version(" a ", "v2", false, false, "2026-11-02T00:00:00Z"),
        version(" a ", "v1", false, false, "2026-11-01T00:00:00Z"),
    ];
    let rule = VersionRule::Cleanup {
        keep: Some(2),
        before: None,
        orphan_markers: false,
    };
    let changes = version_plan(&rows, &rule).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].key, " a ");
    assert_eq!(changes[0].version_id, "v1");
}

#[test]
fn browser_batch_restore_uses_latest_live_version_of_deleted_key() {
    let rows = [
        version("folder/a ", "marker", true, true, "2026-11-03T00:00:00Z"),
        version("folder/a ", "data", false, false, "2026-11-02T00:00:00Z"),
    ];
    let changes = version_plan(&rows, &VersionRule::RestoreDeleted).unwrap();
    assert_eq!(changes[0].version_id, "data");
    assert_eq!(changes[0].expected_latest, "marker");
}

#[test]
fn browser_batch_archive_excludes_case_collisions_and_path_conflicts() {
    let mut names = super::super::files::ArchiveNames::default();
    assert_eq!(
        names.reserve("data/Example.txt").unwrap(),
        "data/Example.txt"
    );
    assert!(names.reserve("DATA/example.TXT").is_err());
    assert!(names.reserve("DATA").is_err());
    assert!(names.reserve("data/Example.txt/child").is_err());
    assert!(names.reserve("../escape").is_err());
    assert!(names.reserve("C:/escape").is_err());
    assert_eq!(names.reserve("empty/").unwrap(), "empty");
    assert!(names.reserve("EMPTY/").is_err());
}

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
async fn browser_batch_expand_nested_selection_deduplicates_literal_keys_across_pages() {
    use crate::object::service::tests::{object_drawer_client, object_drawer_mock};
    let (endpoint, worker) = object_drawer_mock(vec![
        xml(
            "<ListBucketResult><IsTruncated>true</IsTruncated><NextContinuationToken>two</NextContinuationToken><Contents><Key>root/nest/ file </Key><Size>2</Size></Contents></ListBucketResult>",
        ),
        xml(
            "<ListBucketResult><IsTruncated>false</IsTruncated><Contents><Key>root/nest/deep/next</Key><Size>2</Size></Contents></ListBucketResult>",
        ),
    ]);
    let location = Location {
        connection: uuid::Uuid::nil(),
        bucket: "fixture".into(),
        prefix: "root/".into(),
    };
    let objects = vec![ObjectId {
        bucket: "fixture".into(),
        key: "root/nest/ file ".into(),
        version_id: None,
    }];
    let rows = expand_selection(
        &object_drawer_client(&endpoint),
        &location,
        objects,
        vec!["root/nest/deep/".into(), "root/nest/".into()],
    )
    .await
    .unwrap();
    let requests = worker.join().unwrap();
    assert_eq!(
        rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>(),
        ["root/nest/ file ", "root/nest/deep/next"]
    );
    assert!(requests[1].contains("continuation-token=two"));
    assert!(
        requests
            .iter()
            .all(|request| request.contains("prefix=root%2Fnest%2F"))
    );
}

#[tokio::test]
async fn browser_batch_prefix_history_reads_version_and_key_markers() {
    use crate::object::service::tests::{object_drawer_client, object_drawer_mock};
    let (endpoint, worker) = object_drawer_mock(vec![
        xml(
            "<ListVersionsResult><IsTruncated>true</IsTruncated><NextKeyMarker> a </NextKeyMarker><NextVersionIdMarker>v 2</NextVersionIdMarker><Version><Key> a </Key><VersionId>v 2</VersionId><IsLatest>true</IsLatest><LastModified>2026-11-02T00:00:00Z</LastModified><Size>2</Size></Version></ListVersionsResult>",
        ),
        xml(
            "<ListVersionsResult><IsTruncated>false</IsTruncated><Version><Key> a </Key><VersionId>v1</VersionId><IsLatest>false</IsLatest><LastModified>2026-11-01T00:00:00Z</LastModified><Size>1</Size></Version></ListVersionsResult>",
        ),
    ]);
    let location = Location {
        connection: uuid::Uuid::nil(),
        bucket: "fixture".into(),
        prefix: " ".into(),
    };
    let rows = prefix_history(&object_drawer_client(&endpoint), &location)
        .await
        .unwrap();
    let changes = version_plan(
        &rows,
        &VersionRule::Cleanup {
            keep: Some(1),
            before: None,
            orphan_markers: false,
        },
    )
    .unwrap();
    assert_eq!(changes[0].key, " a ");
    assert_eq!(changes[0].version_id, "v1");
    let requests = worker.join().unwrap();
    assert!(requests[1].contains("key-marker=%20a%20"));
    assert!(requests[1].contains("version-id-marker=v%202"));
}
