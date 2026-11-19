use super::repository::provider_result;
use super::*;
use crate::{
    connection::{
        model::{Connection, Secret},
        service::{describe_context, open_session},
    },
    object::service::tests::object_drawer_mock,
};
use reqwest::StatusCode;

fn session(endpoint: &str, identity: IdentityKind) -> Session {
    let profile: Connection = serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(), "revision": Uuid::new_v4(), "name": "Fixture", "provider": "ceph",
        "endpoint": endpoint, "region": "us-east-1", "pathStyle": true, "knownBuckets": [],
        "services": {
            "cephAdmin": { "principalId": Uuid::new_v4(), "endpoint": endpoint, "enabled": true, "capability": "detected", "managedExternally": false },
            "adminOps": { "principalId": Uuid::new_v4(), "endpoint": endpoint, "enabled": true, "capability": "detected", "managedExternally": false }
        }
    }))
    .unwrap();
    let context = describe_context(&profile, Workspace::CephAdmin, identity).unwrap();
    open_session(
        profile,
        context,
        Secret {
            access_key: "fixture-key".into(),
            secret_key: "fixture-secret".into(),
            session_token: None,
        },
    )
    .unwrap()
}

fn response(status: &'static str, body: &str) -> (&'static str, String, Vec<u8>) {
    (
        status,
        format!("Content-Length: {}\r\n", body.len()),
        body.as_bytes().to_vec(),
    )
}

fn bucket(session: &Session) -> BucketTarget {
    BucketTarget {
        connection: session.profile.id,
        revision: session.profile.revision,
        source: InventorySource::CephAdmin,
        tenant: "research".into(),
        name: "records".into(),
        instance: Some("instance-a".into()),
    }
}

#[tokio::test]
async fn rejects_a_stale_account_binding_without_deleting_the_user() {
    let (endpoint, received) = object_drawer_mock(vec![response(
        "200 OK",
        r#"{"user_id":"operator","tenant":"research","account_id":"replacement"}"#,
    )]);
    let session = session(&endpoint, IdentityKind::AdminOps);
    let user = UserIdentity {
        tenant: "research".into(),
        uid: "operator".into(),
    };
    let error = delete_user(
        &session,
        None,
        &user,
        Some("original"),
        false,
        "DELETE USER research$operator",
    )
    .await
    .err()
    .unwrap();
    assert!(error.to_string().contains("account binding changed"));
    assert!(received.join().unwrap()[0].starts_with("GET "));
}

#[tokio::test]
async fn refuses_to_delete_the_executing_user_or_their_account() {
    let user_document = r#"{"user_id":"operator","tenant":"research","account_id":"RGW01234567890123456","keys":[{"access_key":"fixture-key","secret_key":"never-expose"}]}"#;
    let (endpoint, received) = object_drawer_mock(vec![
        response("200 OK", user_document),
        response("200 OK", user_document),
        response(
            "200 OK",
            r#"{"id":"RGW01234567890123456","tenant":"research"}"#,
        ),
        response("200 OK", user_document),
    ]);
    let session = session(&endpoint, IdentityKind::AdminOps);
    let user = UserIdentity {
        tenant: "research".into(),
        uid: "operator".into(),
    };
    let user_error = delete_user(
        &session,
        None,
        &user,
        Some("RGW01234567890123456"),
        true,
        "PURGE USER research$operator",
    )
    .await
    .err()
    .unwrap();
    assert!(user_error.to_string().contains("administrative principal"));
    let account_error = delete_account(
        &session,
        None,
        "RGW01234567890123456",
        "research",
        "DELETE ACCOUNT RGW01234567890123456",
    )
    .await
    .err()
    .unwrap();
    assert!(
        account_error
            .to_string()
            .contains("administrative principal")
    );
    assert!(
        received
            .join()
            .unwrap()
            .iter()
            .all(|request| request.starts_with("GET "))
    );
}

#[tokio::test]
async fn requires_a_current_tenant_and_instance_before_bucket_changes() {
    let (endpoint, received) = object_drawer_mock(vec![response(
        "200 OK",
        r#"{"bucket":"records","tenant":"elsewhere","id":"instance-a","owner":"research$owner"}"#,
    )]);
    let session = session(&endpoint, IdentityKind::CephAdmin);
    let result = change_bucket(
        &session,
        &RgwClient::new(&session).unwrap(),
        &bucket(&session),
        Some("research$owner"),
        BucketChange::Delete {
            purge_objects: true,
            bypass_gc: true,
        },
        "PURGE BUCKET research/records BYPASS GC",
    )
    .await;
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("binding changed")
    );
    assert!(received.join().unwrap()[0].starts_with("GET "));
}

#[tokio::test]
async fn rejects_object_checks_outside_repair_without_network_io() {
    let session = session("http://127.0.0.1:1", IdentityKind::CephAdmin);
    let error = change_bucket(
        &session,
        &RgwClient::new(&session).unwrap(),
        &bucket(&session),
        None,
        BucketChange::Index {
            repair: false,
            check_objects: true,
        },
        "",
    )
    .await
    .err()
    .unwrap();
    assert!(error.to_string().contains("requires index repair"));
}

#[tokio::test]
async fn runs_two_hundred_index_checks_using_read_only_query_parameters() {
    let mut responses = Vec::new();
    for index in 0..200 {
        let name = format!("records-{index:03}");
        let body = format!(r#"{{"bucket":"{name}","tenant":"research","id":"instance-a"}}"#);
        responses.push(response("200 OK", &body));
        responses.push(response("200 OK", r#"{"check_result":{"existing_header":{"num_entries":5},"calculated_header":{"num_entries":4}}}"#));
    }
    let (endpoint, received) = object_drawer_mock(responses);
    let session = session(&endpoint, IdentityKind::CephAdmin);
    let targets = (0..200)
        .map(|index| BucketTarget {
            name: format!("records-{index:03}"),
            ..bucket(&session)
        })
        .collect();
    let mut records = Vec::new();
    check_index_batch(&session, targets, |record| {
        records.push(record);
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(records.len(), 200);
    assert!(records.iter().all(|record| record.result.success));
    let requests = received.join().unwrap();
    assert_eq!(requests.len(), 400);
    assert!(requests.iter().all(|request| {
        let line = request.lines().next().unwrap();
        line.starts_with("GET ") && !line.contains("fix=") && !line.contains("check-objects=")
    }));
}

#[test]
fn reports_rgws_status_and_code_and_projects_only_known_index_fields() {
    let denied = provider_result(
        StatusCode::FORBIDDEN,
        r#"{"Code":"AccessDenied","Message":"fixture-secret","secret_key":"fixture-secret"}"#,
    );
    assert_eq!(denied.status, Some(403));
    assert_eq!(denied.code.as_deref(), Some("AccessDenied"));
    let checked = provider_result(
        StatusCode::OK,
        r#"{"check_result":{"existing_header":{"num_entries":5},"calculated_header":{"num_entries":4}},"secret_key":"fixture-secret"}"#,
    );
    assert_eq!(
        checked.details.as_ref().unwrap()["check_result"]["calculated_header"]["num_entries"],
        4
    );
    assert!(
        !serde_json::to_string(&checked)
            .unwrap()
            .contains("fixture-secret")
    );
    assert!(
        !serde_json::to_string(&denied)
            .unwrap()
            .contains("fixture-secret")
    );
}
