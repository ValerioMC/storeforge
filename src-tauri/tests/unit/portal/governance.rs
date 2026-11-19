use serde_json::json;
use tempfile::tempdir;

use super::{
    model::{AdminRequestQuery, AdminResponse, GovernanceRead, PortalRead},
    portal_spaces::{Request, serve, serve_as},
    service::{self, governance},
};
use crate::{
    common::error::AppError,
    connection::shared_service::{model::ServiceDraft, service::SharedService},
};

#[tokio::test]
async fn portal_governance_validates_scopes_and_keeps_approval_with_authority() {
    let directory = tempdir().unwrap();
    let (member_origin, member_token, member_worker) = serve(vec![
        Request::new("POST", "/api/portal/requests?account_id=12", 201, r#"{"id":7,"status":"pending"}"#).body(json!({
            "request_type":"account_quota_change", "direction":"increase", "target_quota_value":50.0,
            "target_quota_unit":"GiB", "reason":"More storage"
        })),
        Request::new("GET", "/api/portal/access-logs/page?account_id=12&date=2026-11-19&offset=0&limit=100&timezone_offset_minutes=60", 403, "{}"),
        Request::new("GET", "/api/portal/requests?account_id=12", 200, r#"[{"id":7,"status":"approved","messages":[{"message":"Approved"}]}]"#),
        Request::new("PATCH", "/api/portal/settings?account_id=12", 200, r#"{"can_update":true}"#)
            .body(json!({"bucket_defaults":{"noncurrent_version_expiration_days":null}})),
    ]);
    let (admin_origin, admin_token, admin_worker) = serve_as(
        vec![
            Request::new(
                "GET",
                "/api/admin/portal-requests?page=1&page_size=25&search=",
                200,
                r#"{"items":[{"id":7,"status":"pending"}]}"#,
            ),
            Request::new(
                "POST",
                "/api/admin/portal-requests/7/approve",
                200,
                r#"{"id":7,"status":"approved"}"#,
            )
            .body(json!({"message":"Approved"})),
            Request::new("POST", "/api/admin/portal-requests/7/approve", 409, "{}")
                .body(json!({"message":"Approved"})),
        ],
        true,
    );
    let shared = SharedService::open(directory.path().into()).unwrap();
    let member = shared
        .save(
            None,
            ServiceDraft {
                name: "Member".into(),
                origin: member_origin,
            },
        )
        .unwrap();
    let admin = shared
        .save(
            None,
            ServiceDraft {
                name: "Admin".into(),
                origin: admin_origin,
            },
        )
        .unwrap();
    shared.connect(member.id, member_token).await.unwrap();
    shared.connect(admin.id, admin_token).await.unwrap();
    let requested = service::change(
        &shared,
        member.id,
        12,
        None,
        serde_json::from_value(json!({
            "kind":"governance", "action":{"kind":"requestQuota","direction":"increase","size":50,
                "unit":"GiB","reason":"More storage"}
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(requested["status"], "pending");
    let query = || AdminRequestQuery {
        page: 1,
        page_size: 25,
        search: String::new(),
        status: None,
        request_type: None,
        account_id: None,
    };
    assert!(matches!(
        governance::admin_requests(&shared, member.id, query()).await,
        Err(AppError::AuthorizationDenied(_))
    ));
    let logs = serde_json::from_value::<GovernanceRead>(json!({
        "kind":"accessLogs", "query":{"date":"2026-11-19","offset":0,"timezone_offset_minutes":60}
    }))
    .unwrap();
    assert!(matches!(
        service::read(
            &shared,
            member.id,
            Some(12),
            None,
            PortalRead::Governance { view: logs }
        )
        .await,
        Err(AppError::AuthorizationDenied(_))
    ));
    let rows = governance::admin_requests(&shared, admin.id, query())
        .await
        .unwrap();
    assert_eq!(rows["items"][0]["status"], "pending");
    let response = || AdminResponse::Approve {
        message: Some("Approved".into()),
    };
    let approved = governance::respond(&shared, admin.id, 7, response())
        .await
        .unwrap();
    assert_eq!(approved["status"], "approved");
    assert!(
        governance::respond(&shared, admin.id, 7, response())
            .await
            .is_err()
    );
    let requester = service::read(
        &shared,
        member.id,
        Some(12),
        None,
        serde_json::from_value(json!({
            "kind":"sharing","view":{"kind":"requests"}
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(requester[0]["status"], "approved");
    assert_eq!(requester[0]["messages"].as_array().unwrap().len(), 1);
    service::change(&shared, member.id, 12, None, serde_json::from_value(json!({
        "kind":"governance", "action":{"kind":"saveSetting","setting":{"key":"bucket_defaults.noncurrent_version_expiration_days","value":null}}
    })).unwrap()).await.unwrap();
    member_worker.join().unwrap();
    admin_worker.join().unwrap();
}
