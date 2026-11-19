use serde_json::json;
use tempfile::tempdir;

use super::{
    model::{PortalChange, PortalRead, SharingRead},
    portal_spaces::{Request, serve},
    service,
};
use crate::{
    common::error::AppError,
    connection::shared_service::{model::ServiceDraft, service::SharedService},
};

#[tokio::test]
async fn portal_sharing_projects_permissions_and_filters_runtime_keys_and_secrets() {
    let directory = tempdir().unwrap();
    let key = json!({
        "access_key_id":"AK-EXT", "target_type":"external", "status":"Active",
        "is_active":true, "is_portal":false, "deletable":true, "bucket_name":"docs-data",
        "permission":"read_only", "secret_access_key":"persisted-secret", "session_token":"persisted-token"
    });
    let runtime_key = json!({
        "access_key_id":"server-only", "target_type":"self", "is_active":true, "is_portal":true, "deletable":false
    });
    let keys = json!({
        "s3_endpoint":"https://s3.example.test", "force_path_style":true, "access_keys":[key.clone(),runtime_key],
        "can_manage_access_keys":true, "can_create_external_access":true, "max_access_keys":2
    });
    let mut receipt = key;
    receipt["secret_access_key"] = json!("one-time");
    let (origin, token, worker) = serve(vec![
        Request::new("GET", "/api/portal/collaborators/8/access?account_id=12", 200,
            r#"{"space_accesses":[{"role":"Editor","source":"direct"}]}"#),
        Request::new("GET", "/api/portal/collaborators/8/access?account_id=34", 403, "{}"),
        Request::new("DELETE", "/api/portal/storage-spaces/docs/public-links/3?account_id=12", 200, "[]"),
        Request::new("GET", "/api/portal/storage-spaces/docs/public-links?account_id=12&include_revoked=true", 200,
            r#"[{"id":3,"status":"Revoked"}]"#),
        Request::new("GET", "/api/portal/access-keys?account_id=12", 200, keys.to_string()),
        Request::new("GET", "/api/portal/access-keys?account_id=12", 200, keys.to_string()),
        Request::new("POST", "/api/portal/access-keys?account_id=12", 201, receipt.to_string()).body(json!({
            "target_type":"external", "storage_space_id":"docs", "external_email":"Partner", "permission":"read_only"
        })),
    ]);
    let shared = SharedService::open(directory.path().into()).unwrap();
    let profile = shared
        .save(
            None,
            ServiceDraft {
                name: "Project".into(),
                origin,
            },
        )
        .unwrap();
    shared.connect(profile.id, token).await.unwrap();
    let collaborator = || PortalRead::Sharing {
        view: SharingRead::Collaborator { user_id: 8 },
    };
    let review = service::read(&shared, profile.id, Some(12), None, collaborator())
        .await
        .unwrap();
    assert_eq!(review["space_accesses"][0]["role"], "Editor");
    assert!(matches!(
        service::read(&shared, profile.id, Some(34), None, collaborator()).await,
        Err(AppError::AuthorizationDenied(_))
    ));
    service::change(
        &shared,
        profile.id,
        12,
        Some("docs".into()),
        request(json!({"kind":"revokeLink","link_id":3})),
    )
    .await
    .unwrap();
    let links = service::read(
        &shared,
        profile.id,
        Some(12),
        Some("docs".into()),
        PortalRead::Sharing {
            view: SharingRead::Links,
        },
    )
    .await
    .unwrap();
    assert_eq!(links[0]["status"], "Revoked");
    let keys = service::read(
        &shared,
        profile.id,
        Some(12),
        None,
        PortalRead::Sharing {
            view: SharingRead::Keys,
        },
    )
    .await
    .unwrap();
    assert_eq!(keys["access_keys"].as_array().unwrap().len(), 1);
    assert_eq!(keys["access_keys"][0]["access_key_id"], "AK-EXT");
    assert!(!keys.to_string().contains("persisted-"));
    assert!(matches!(
        service::change(
            &shared,
            profile.id,
            12,
            None,
            request(json!({"kind":"deleteKey","key_id":"server-only"}))
        )
        .await,
        Err(AppError::AuthorizationDenied(_))
    ));
    let created = service::change(&shared, profile.id, 12, None, request(json!({
        "kind":"externalKey", "storage_space_id":"docs", "external_email":"Partner", "permission":"read_only"
    }))).await.unwrap();
    assert_eq!(created["secret_access_key"], "one-time");
    assert!(created.get("session_token").is_none());
    worker.join().unwrap();
}

fn request(action: serde_json::Value) -> PortalChange {
    serde_json::from_value(json!({"kind":"sharing","action":action})).unwrap()
}
