use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde_json::{Value, json};
use tempfile::tempdir;

use crate::{
    common::error::AppError,
    connection::shared_service::{model::ServiceDraft, service::SharedService},
};

use super::{
    model::{
        PortalFlag, RotationKind, RotationPhase, RotationRecovery, RotationStatus, SettingEdit,
        SettingValue, SettingsSection,
    },
    service::WorkspaceSettingsService,
};

const PRINCIPAL: &str =
    r#"{"id":17,"email":"admin@example.org","role":"ui_superadmin","is_active":true}"#;
const ACCESS: &str = r#"{"admin":{"available":true,"context_count":1},"browser":{"available":false,"context_count":0},"manager":{"available":false,"context_count":0},"portal":{"available":false,"context_count":0},"storage_ops":{"available":false,"context_count":0},"ceph_admin":{"available":false,"context_count":0}}"#;
const PORTAL: &str = r#"{"admin_override":{"browser_access_enabled":true,"bucket_defaults":{"versioning":true}},"effective":{"browser_access_enabled":false}}"#;
const ENDPOINT: &str = r#"{"id":12,"name":"East","provider":"ceph","is_editable":true,"capabilities":{"admin":true},"service_identities":[{"kind":"runtime","mode":"managed","status":"ready","rotation_phase":"prepared"}]}"#;
const ENDPOINT_COMPLETE: &str = r#"{"id":12,"name":"East","provider":"ceph","is_editable":true,"capabilities":{"admin":true},"service_identities":[{"kind":"runtime","mode":"managed","status":"ready","rotation_phase":null}]}"#;
const RESULTS: &str = r#"{"summary":{"rotated":1,"failed":1,"skipped":1,"deleted_old_keys":1,"disabled_old_keys":0},"results":[{"endpoint_id":12,"endpoint_name":"East","key_type":"endpoint_runtime","target_id":"12","target_label":"East runtime","status":"rotated","message":"Revalidated and retired","rotation_phase":null},{"endpoint_id":13,"endpoint_name":"West","key_type":"endpoint_runtime","target_id":"13","target_label":"West runtime","status":"failed","message":"Validation failed","rotation_phase":"prepared"},{"endpoint_id":14,"endpoint_name":"Archive","key_type":"endpoint_runtime","target_id":null,"target_label":null,"status":"skipped","message":"External credential","rotation_phase":null}]}"#;

#[tokio::test]
async fn settings_rotation_preserves_a_portal_override_under_a_platform_prohibition() {
    let (origin, worker) = fixtures(vec![(
        "GET /api/admin/accounts/23/portal-settings",
        PORTAL,
        None,
    )]);
    let directory = tempdir().unwrap();
    let (shared, id) = connect(directory.path(), origin).await;
    let service = WorkspaceSettingsService::new(&shared, id);
    let result = service.portal_account(23, None).await.unwrap();

    assert!(matches!(
        result.admin_override["browser_access_enabled"],
        PortalFlag::Switch(true)
    ));
    assert!(matches!(
        result.effective["browser_access_enabled"],
        PortalFlag::Switch(false)
    ));
    worker.join().unwrap();
}

#[tokio::test]
async fn settings_rotation_resumes_only_the_requested_pending_category() {
    let (origin, worker) = fixtures(vec![
        ("GET /api/admin/storage-endpoints/12", ENDPOINT, None),
        (
            "POST /api/admin/key-rotation",
            RESULTS,
            Some(
                json!({"endpoint_ids":[12],"key_types":["endpoint_runtime"],"deactivate_only":false}),
            ),
        ),
        (
            "GET /api/admin/storage-endpoints/12",
            ENDPOINT_COMPLETE,
            None,
        ),
    ]);
    let directory = tempdir().unwrap();
    let (shared, id) = connect(directory.path(), origin).await;
    let service = WorkspaceSettingsService::new(&shared, id);
    let recovery = || RotationRecovery {
        endpoint_id: 12,
        kind: RotationKind::EndpointRuntime,
        phase: RotationPhase::Prepared,
        deactivate_only: false,
    };
    let report = service.recover(recovery()).await.unwrap();

    assert!(matches!(report.results[0].status, RotationStatus::Rotated));
    assert!(matches!(report.results[1].status, RotationStatus::Failed));
    assert!(matches!(report.results[2].status, RotationStatus::Skipped));
    assert!(matches!(
        service.recover(recovery()).await,
        Err(AppError::Invalid(_))
    ));
    worker.join().unwrap();
}

#[tokio::test]
async fn settings_rotation_saves_only_reviewed_fields_and_preserves_other_settings() {
    let settings = r#"{"browser":{"allow_direct_uploads":true,"proxy_upload_parallelism":4},"branding":{"primary_color":"blue"}}"#;
    let saved = r#"{"browser":{"allow_direct_uploads":false,"proxy_upload_parallelism":4},"branding":{"primary_color":"blue"}}"#;
    let (origin, worker) = fixtures(vec![
        ("GET /api/admin/settings", settings, None),
        (
            "PUT /api/admin/settings",
            saved,
            Some(serde_json::from_str(saved).unwrap()),
        ),
    ]);
    let directory = tempdir().unwrap();
    let (shared, id) = connect(directory.path(), origin).await;
    let service = WorkspaceSettingsService::new(&shared, id);
    let result = service
        .save(
            SettingsSection::Browser,
            vec![SettingEdit {
                group: None,
                field: "allow_direct_uploads".into(),
                before: SettingValue::Switch(true),
                value: SettingValue::Switch(false),
            }],
        )
        .await
        .unwrap();

    assert!(matches!(
        result.groups[0].values["allow_direct_uploads"],
        SettingValue::Switch(false)
    ));
    worker.join().unwrap();
}

async fn connect(directory: &std::path::Path, origin: String) -> (SharedService, uuid::Uuid) {
    let shared = SharedService::open(directory.into()).unwrap();
    let profile = shared
        .save(
            None,
            ServiceDraft {
                name: "Fixture".into(),
                origin,
            },
        )
        .unwrap();
    let exp = Utc::now().timestamp() + 3_600;
    let claims = URL_SAFE_NO_PAD.encode(json!({"typ":"api_access","exp":exp}).to_string());
    shared
        .connect(profile.id, format!("e30.{claims}.fixture"))
        .await
        .unwrap();
    (shared, profile.id)
}

type Fixture = (&'static str, &'static str, Option<Value>);

fn fixtures(mut routes: Vec<Fixture>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let mut requests = vec![
            ("GET /api/users/me", PRINCIPAL, None),
            ("GET /api/me/workspace-access", ACCESS, None),
        ];
        requests.append(&mut routes);
        for (target, body, expected_payload) in requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut input = BufReader::new(&mut stream);
            let mut line = String::new();
            input.read_line(&mut line).unwrap();
            assert_eq!(line.trim_end(), format!("{target} HTTP/1.1"));
            let headers = read_headers(&mut input);
            assert!(headers.contains("authorization: Bearer "));
            assert!(!headers.contains("cookie:"));
            if let Some(expected) = expected_payload {
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                let mut bytes = vec![0; length];
                input.read_exact(&mut bytes).unwrap();
                assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
            }
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (origin, worker)
}

fn read_headers(input: &mut impl BufRead) -> String {
    let mut headers = String::new();
    loop {
        let count = input.read_line(&mut headers).unwrap();
        if count <= 2 {
            return headers;
        }
    }
}
