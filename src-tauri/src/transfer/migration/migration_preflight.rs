use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    common::error::AppError,
    connection::{
        model::{Connection, IdentityKind, Secret, Workspace},
        service::{Session, describe_context, open_session},
    },
    object::service::tests::{object_drawer_client, object_drawer_mock},
};

use super::{
    model::{Draft, Mapping, Method, Migration, Mode, Phase, Report, Side},
    preflight::{protect, require_absent, restore_effect},
    repository::MigrationRepository,
};

pub(super) fn fixture(endpoint: &str) -> Session {
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
            session_token: Some("fixture-session-token".into()),
        },
    )
    .unwrap();
    session.s3 = object_drawer_client(endpoint);
    session
}

pub(super) fn draft(source: &Session, target: &Session) -> Draft {
    Draft {
        source: source.profile.id,
        target: target.profile.id,
        mappings: vec![Mapping {
            source: "source-bucket".into(),
            target: "new-bucket".into(),
        }],
        mode: Mode::PreCopy,
        method: Method::Streamed,
        history: true,
        settings: false,
        parallelism: 2,
        protect_destination: false,
        grant_source_read: false,
    }
}

pub(super) fn response(status: &'static str, body: &str) -> (&'static str, String, Vec<u8>) {
    (
        status,
        format!("Content-Length: {}\r\n", body.len()),
        body.as_bytes().to_vec(),
    )
}

#[test]
fn draft_survives_restart_and_review_is_bound_to_revision_and_expiry() {
    let directory = tempfile::tempdir().unwrap();
    let session = fixture("http://127.0.0.1:1");
    let mut draft = draft(&session, &session);
    assert!(draft.validate(&session, &session).is_ok());
    draft.mappings[0].target = draft.mappings[0].source.clone();
    assert!(draft.validate(&session, &session).is_err());
    draft.mappings[0].target = "new-bucket".into();
    let mut record = Migration::new(Uuid::new_v4(), draft, &session, &session);
    let mut report = Report::new(record.revision, Phase::Active, Vec::new());
    assert!(report.passed(record.revision, Phase::Active));
    assert!(!report.passed(Uuid::new_v4(), Phase::Active));
    report.expires_at = Utc::now() - Duration::seconds(1);
    assert!(!report.passed(record.revision, Phase::Active));
    record.report = Some(report);
    let repository = MigrationRepository::open(directory.path().into()).unwrap();
    repository.save(&mut record).unwrap();
    let stored = std::fs::read_to_string(directory.path().join("migrations.json")).unwrap();
    for secret in ["fixture-key", "fixture-secret", "fixture-session-token"] {
        assert!(!stored.contains(secret));
    }
    let reopened = MigrationRepository::open(directory.path().into()).unwrap();
    let reopened = reopened.get(record.id).unwrap();
    assert_eq!(record.revision, reopened.revision);
    assert_eq!(record.draft.mappings, reopened.draft.mappings);
}

#[tokio::test]
async fn existing_empty_destination_and_denied_destination_are_blocked() {
    let (endpoint, server) = object_drawer_mock(vec![
        response("200 OK", ""),
        response("403 Forbidden", ""),
        response("404 Not Found", ""),
    ]);
    let session = fixture(&endpoint);
    assert!(matches!(
        require_absent(&session.s3, "empty").await,
        Err(AppError::Invalid(_))
    ));
    assert!(matches!(
        require_absent(&session.s3, "denied").await,
        Err(AppError::Storage(_))
    ));
    assert!(require_absent(&session.s3, "absent").await.is_ok());
    let requests = server.join().unwrap();
    assert!(requests.iter().all(|request| request.starts_with("HEAD ")));
}

#[tokio::test]
async fn failed_policy_restore_survives_restart_and_can_be_retried() {
    let original = r#"{"Version":"2012-10-17","Statement":[]}"#;
    let (endpoint, server) = object_drawer_mock(vec![
        response("200 OK", original),
        response("204 No Content", ""),
        response("403 Forbidden", "<Error><Code>AccessDenied</Code></Error>"),
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
    record.checking = Some(Phase::Active);
    assert!(
        protect(
            &repository,
            &mut record,
            &session,
            Side::Source,
            "source-bucket",
            None,
            None
        )
        .await
        .is_err()
    );
    server.join().unwrap();
    let repository = MigrationRepository::open(directory.path().into()).unwrap();
    let record = repository.get(record.id).unwrap();
    assert!(record.checking.is_none());
    assert!(record.require_recovered().is_err());
    let super::model::Effect::Policy { protection, .. } = &record.private.effects[0] else {
        panic!("Expected policy receipt")
    };
    let protected = protection.to_string();
    let (endpoint, server) = object_drawer_mock(vec![
        response("200 OK", &protected),
        response("403 Forbidden", "<Error><Code>AccessDenied</Code></Error>"),
        response("200 OK", &protected),
        response("204 No Content", ""),
        response("200 OK", original),
    ]);
    let session = fixture(&endpoint);
    let effect = &record.private.effects[0];
    assert!(restore_effect(effect, &session, &session).await.is_err());
    assert!(restore_effect(effect, &session, &session).await.is_ok());
    let requests = server.join().unwrap();
    assert!(requests[3].starts_with("PUT "));
    assert!(requests[3].contains("/source-bucket"));
    assert!(requests[3].contains("policy"));
}
