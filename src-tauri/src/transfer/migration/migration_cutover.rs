use std::collections::BTreeMap;

use uuid::Uuid;

use crate::object::service::tests::object_drawer_mock;

use super::{
    execution::Execution,
    execution_model::{BucketStatus, Fingerprint, Ownership, Run, Status},
    history,
    migration_preflight::{draft, fixture, response},
    model::{Effect, Migration, PairPlan, Phase, Report, Side},
    ownership::{self, RECEIPT_TAG},
    preflight::{PolicyAccess, build_protection},
    replay,
    repository::MigrationRepository,
};

fn review(record: &mut Migration) {
    record.report = Some(Report::new(
        record.revision,
        Phase::Active,
        record
            .draft
            .mappings
            .iter()
            .cloned()
            .map(|mapping| PairPlan {
                mapping,
                sample: None,
                settings_copied: Vec::new(),
                settings_omitted: Vec::new(),
                issues: Vec::new(),
            })
            .collect(),
    ));
    record.run = Some(Run::start(record).unwrap());
}

fn tagging(token: Uuid) -> String {
    format!(
        "<Tagging><TagSet><Tag><Key>{RECEIPT_TAG}</Key><Value>{token}</Value></Tag></TagSet></Tagging>"
    )
}

fn versions(entries: &str) -> String {
    format!("<ListVersionsResult><IsTruncated>false</IsTruncated>{entries}</ListVersionsResult>")
}

fn version(key: &str, version: &str, latest: bool) -> String {
    format!(
        "<Version><Key>{key}</Key><VersionId>{version}</VersionId><IsLatest>{latest}</IsLatest></Version>"
    )
}

#[tokio::test]
async fn restart_after_creation_resumes_from_receipt_without_creating_another_bucket() {
    let token = Uuid::new_v4();
    let receipt = tagging(token);
    let (endpoint, server) = object_drawer_mock(vec![
        response("404 Not Found", "<Error><Code>NoSuchTagSet</Code></Error>"),
        response("200 OK", &versions("")),
        response("200 OK", ""),
        response("200 OK", &receipt),
    ]);
    let session = fixture(&endpoint);
    let directory = tempfile::tempdir().unwrap();
    let repository = MigrationRepository::open(directory.path().into()).unwrap();
    let mut record = Migration::new(
        Uuid::new_v4(),
        draft(&session, &session),
        &session,
        &session,
    );
    review(&mut record);
    record.run.as_mut().unwrap().buckets[0].ownership = Ownership::Created { token };
    repository.save(&mut record).unwrap();
    let reopened = MigrationRepository::open(directory.path().into()).unwrap();
    let mut record = reopened.get(record.id).unwrap();
    assert_eq!(record.run.as_ref().unwrap().status, Status::Paused);
    ownership::prepare(&reopened, &mut record, &session, 0)
        .await
        .unwrap();
    assert!(record.run.as_ref().unwrap().buckets[0].ownership == Ownership::Owned { token });
    let requests = server.join().unwrap();
    assert!(requests[0].lines().next().unwrap().contains("tagging"));
    assert!(requests[1].lines().next().unwrap().contains("versions"));
    assert!(requests[2].starts_with("PUT ") && requests[2].contains("tagging"));
    assert!(requests[3].lines().next().unwrap().contains("tagging"));
}

#[tokio::test]
async fn history_preserves_provider_order_and_exact_keys_across_pages() {
    let key = "%20a%2B%20";
    let first = format!(
        "<ListVersionsResult><EncodingType>url</EncodingType><IsTruncated>true</IsTruncated><NextKeyMarker>{key}</NextKeyMarker><NextVersionIdMarker>marker</NextVersionIdMarker>{}<DeleteMarker><Key>{key}</Key><VersionId>marker</VersionId><IsLatest>false</IsLatest></DeleteMarker></ListVersionsResult>",
        version(key, "current", true)
    );
    let second = format!(
        "<ListVersionsResult><EncodingType>url</EncodingType><IsTruncated>false</IsTruncated>{}</ListVersionsResult>",
        version(key, "old", false)
    );
    let (endpoint, server) = object_drawer_mock(vec![
        response("200 OK", &first),
        response("200 OK", &second),
    ]);
    let session = fixture(&endpoint);
    let history = history::inventory(&session, "bucket", true).await.unwrap();
    let entries = &history[" a+ "];
    assert_eq!(
        entries
            .iter()
            .map(|entry| (entry.object.version_id.as_deref().unwrap(), entry.marker))
            .collect::<Vec<_>>(),
        vec![("old", false), ("marker", true), ("current", false)]
    );
    let requests = server.join().unwrap();
    assert!(requests[1].contains("version-id-marker=marker"));
    assert!(requests[1].contains("key-marker=+a%2B+"));
}

#[test]
fn retry_skips_matching_ordered_history_and_refuses_unowned_divergence() {
    let data = |sha256: &str, tag: &str| Fingerprint::Data {
        sha256: sha256.into(),
        tags: vec![(" label ".into(), tag.into())],
    };
    let old = data("first", " exact ");
    let new = data("second", "next");
    let changed_tag = data("first", "exact");
    let source = [old.clone(), Fingerprint::Marker, new.clone()];
    assert_eq!(
        replay::reviewed_prefix(&source, &source[..2], &[]).unwrap(),
        2
    );
    assert!(replay::reviewed_prefix(&source, &[changed_tag], &[]).is_err());
    assert!(replay::reviewed_prefix(&source, &[Fingerprint::Marker, old.clone()], &[]).is_err());
    assert_eq!(
        replay::reviewed_prefix(
            &source,
            &[old, new],
            &[data("first", " exact "), data("second", "next")]
        )
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn fresh_cleanup_rejects_changed_destination_and_restores_its_policy() {
    let token = Uuid::new_v4();
    let directory = tempfile::tempdir().unwrap();
    let repository = MigrationRepository::open(directory.path().into()).unwrap();
    let source_listing = versions(&version("key", "source-id", true));
    let target_listing = versions(&version("key", "dest-id", true));
    let empty_tags = "<Tagging><TagSet/></Tagging>";
    let original = serde_json::json!({"Version":"2012-10-17","Statement":[]});
    let mock = fixture("http://127.0.0.1:1");
    let id = Uuid::new_v4();
    let read_only = || PolicyAccess {
        deny_writes: true,
        writer: None,
        read_grant: None,
        delete_only: false,
    };
    let source_protection = build_protection(
        Some(original.clone()),
        id,
        &mock,
        "source-bucket",
        read_only(),
    )
    .unwrap();
    let target_protection =
        build_protection(Some(original.clone()), id, &mock, "new-bucket", read_only()).unwrap();
    let (source_endpoint, source_server) = object_drawer_mock(vec![
        response("200 OK", &source_protection.to_string()),
        response("200 OK", &source_listing),
        response("200 OK", "old"),
        response("200 OK", empty_tags),
    ]);
    let (target_endpoint, target_server) = object_drawer_mock(vec![
        response("200 OK", &tagging(token)),
        response("200 OK", &target_protection.to_string()),
        response("200 OK", &target_protection.to_string()),
        response("200 OK", &target_listing),
        response("200 OK", "new"),
        response("200 OK", empty_tags),
        response("200 OK", &target_protection.to_string()),
        response("204 No Content", ""),
        response("200 OK", &original.to_string()),
    ]);
    let source = fixture(&source_endpoint);
    let target = fixture(&target_endpoint);
    let mut record = Migration::new(id, draft(&source, &target), &source, &target);
    review(&mut record);
    let run = record.run.as_mut().unwrap();
    run.status = Status::Verified;
    let bucket = &mut run.buckets[0];
    bucket.status = BucketStatus::Verified;
    bucket.ownership = Ownership::Owned { token };
    bucket.baseline = BTreeMap::new();
    record.private.effects = vec![
        Effect::Policy {
            side: Side::Source,
            bucket: "source-bucket".into(),
            original: Some(original.clone()),
            protection: source_protection,
        },
        Effect::Policy {
            side: Side::Target,
            bucket: "new-bucket".into(),
            original: Some(original),
            protection: target_protection,
        },
    ];
    Execution {
        repository: &repository,
        source: &source,
        target: &target,
    }
    .delete_sources(&mut record, &[0])
    .await
    .unwrap();
    let bucket = &record.run.as_ref().unwrap().buckets[0];
    assert!(!bucket.source_deleted);
    assert!(
        bucket
            .cleanup_error
            .as_ref()
            .unwrap()
            .contains("fresh content")
    );
    assert_eq!(record.private.effects.len(), 1);
    assert!(
        source_server
            .join()
            .unwrap()
            .iter()
            .all(|request| request.starts_with("GET "))
    );
    assert!(target_server.join().unwrap()[7].starts_with("PUT "));
}

#[tokio::test]
async fn restart_after_source_protection_retains_the_receipt_and_restore_preserves_completed_data()
{
    let directory = tempfile::tempdir().unwrap();
    let repository = MigrationRepository::open(directory.path().into()).unwrap();
    let id = Uuid::new_v4();
    let mock = fixture("http://127.0.0.1:1");
    let original = serde_json::json!({"Version":"2012-10-17","Statement":[]});
    let protection = build_protection(
        Some(original.clone()),
        id,
        &mock,
        "source-bucket",
        PolicyAccess {
            deny_writes: true,
            writer: None,
            read_grant: None,
            delete_only: false,
        },
    )
    .unwrap();
    let (endpoint, server) = object_drawer_mock(vec![
        response("200 OK", &protection.to_string()),
        response("204 No Content", ""),
        response("200 OK", &original.to_string()),
    ]);
    let source = fixture(&endpoint);
    let target = mock;
    let mut record = Migration::new(id, draft(&source, &target), &source, &target);
    review(&mut record);
    let protected_bucket = &mut record.run.as_mut().unwrap().buckets[0];
    protected_bucket.status = BucketStatus::Verified;
    protected_bucket.ownership = Ownership::Owned {
        token: Uuid::new_v4(),
    };
    record.private.effects.push(Effect::Policy {
        side: Side::Source,
        bucket: "source-bucket".into(),
        original: Some(original),
        protection,
    });
    repository.save(&mut record).unwrap();
    let reopened = MigrationRepository::open(directory.path().into()).unwrap();
    let mut record = reopened.get(id).unwrap();
    assert_eq!(record.run.as_ref().unwrap().status, Status::Paused);
    assert_eq!(record.private.effects.len(), 1);
    assert!(
        Execution {
            repository: &reopened,
            source: &source,
            target: &target
        }
        .restore(&mut record, |_| true)
        .await
        .unwrap()
    );
    assert!(record.private.effects.is_empty());
    assert_eq!(
        record.run.as_ref().unwrap().buckets[0].status,
        BucketStatus::Verified
    );
    assert!(matches!(
        record.run.as_ref().unwrap().buckets[0].ownership,
        Ownership::Owned { .. }
    ));
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[1].starts_with("PUT "));
}

#[tokio::test]
async fn fingerprints_include_literal_tags_and_sha256_of_exact_noncurrent_version() {
    let tagging =
        "<Tagging><TagSet><Tag><Key> name </Key><Value> value </Value></Tag></TagSet></Tagging>";
    let (endpoint, server) =
        object_drawer_mock(vec![response("200 OK", "abc"), response("200 OK", tagging)]);
    let session = fixture(&endpoint);
    let entry = history::Entry {
        object: crate::object::model::ObjectId {
            bucket: "source-bucket".into(),
            key: " key ".into(),
            version_id: Some("earlier /+".into()),
        },
        marker: false,
    };
    assert_eq!(
        replay::fingerprint(&session.s3, &entry).await.unwrap(),
        Fingerprint::Data {
            sha256: "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=".into(),
            tags: vec![(" name ".into(), " value ".into())],
        }
    );
    let requests = server.join().unwrap();
    assert!(
        requests
            .iter()
            .all(|request| request.contains("versionId=earlier%20%2F%2B"))
    );
}
