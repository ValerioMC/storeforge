use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    thread,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde_json::json;
use tempfile::tempdir;

use crate::connection::shared_service::{model::ServiceDraft, service::SharedService};

use super::{
    model::{EventStatus, HistoryFilter, HistoryScope, WebhookStatus},
    repository::OperationsRepository,
    service::OperationsService,
};

const PRINCIPAL: &str =
    r#"{"id":17,"email":"admin@example.org","role":"ui_superadmin","is_active":true}"#;
const ACCESS: &str = r#"{"admin":{"available":true,"context_count":1},"browser":{"available":false,"context_count":0},"manager":{"available":true,"context_count":2},"portal":{"available":false,"context_count":0},"storage_ops":{"available":false,"context_count":0},"ceph_admin":{"available":false,"context_count":0}}"#;
const MANAGER_JSON: &str = r#"[{"id":23,"created_at":"2026-11-19T10:50:00Z","user_email":"user@example.org","action":"bucket.update","entity_type":"bucket","entity_id":"photos","context_name":"Research","status":"partial"}]"#;
const ADMIN_NONMATCH: &str = r#"{"logs":[{"id":22,"created_at":"2026-11-19T10:50:00Z","user_email":"user@example.org","action":"settings.update","entity_type":"settings","entity_id":"platform","context_name":null,"status":"success"}],"next_cursor":22}"#;
const ADMIN_MATCH: &str = r#"{"logs":[{"id":18,"created_at":"2026-11-19T10:49:00Z","user_email":"admin@example.org","action":"settings.update","entity_type":"settings","entity_id":"platform","context_name":null,"status":"failure","scope":"admin","message":"An upstream response was denied"}],"next_cursor":null}"#;
const RETRY: &str = r#"[{"id":39,"event_type":"admin.settings.update","is_test":false,"status":"retrying","attempt_count":1,"last_http_status":429,"last_error":"Rate limited","updated_at":"2026-11-19T10:52:00Z","delivered_at":null}]"#;
const DELIVERED: &str = r#"[{"id":39,"event_type":"admin.settings.update","is_test":false,"status":"delivered","attempt_count":2,"last_http_status":204,"last_error":null,"updated_at":"2026-11-19T10:55:00Z","delivered_at":"2026-11-19T10:55:00Z"}]"#;

#[tokio::test]
async fn operational_events_filters_admin_across_pages_and_preserves_manager_context() {
    let (origin, worker) = fixtures(vec![
        ("GET /api/admin/audit/logs?limit=20", ADMIN_NONMATCH),
        ("GET /api/admin/audit/logs?limit=20&cursor=22", ADMIN_MATCH),
        (
            "GET /api/manager/activity?limit=20&context_id=connection%3A101&search=photos&status=partial",
            MANAGER_JSON,
        ),
    ]);
    let directory = tempdir().unwrap();
    let (shared, id) = connect(directory.path(), origin).await;
    let service = OperationsService::new(&shared, id);
    let page = service
        .history(
            HistoryScope::Admin,
            HistoryFilter {
                actor: Some("ADMIN@example.org".into()),
                status: Some(EventStatus::Failure),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.logs.len(), 1);
    assert_eq!(page.logs[0].id, 18);
    assert_eq!(page.logs[0].scope.as_deref(), Some("admin"));

    let page = service
        .history(
            HistoryScope::Manager,
            HistoryFilter {
                context_id: Some("connection:101".into()),
                search: Some("photos".into()),
                status: Some(EventStatus::Partial),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.logs[0].context_name.as_deref(), Some("Research"));
    assert_eq!(page.logs[0].entity_id.as_deref(), Some("photos"));
    worker.join().unwrap();
}

#[tokio::test]
async fn operational_events_keeps_the_delivery_and_original_event_across_retries() {
    let path = "GET /api/admin/settings/webhooks/7/deliveries?limit=100";
    let (origin, worker) = fixtures(vec![(path, RETRY), (path, DELIVERED)]);
    let directory = tempdir().unwrap();
    let (shared, id) = connect(directory.path(), origin).await;
    let repository = OperationsRepository::new(&shared, id);
    let before = repository.deliveries(7).await.unwrap();
    let after = repository.deliveries(7).await.unwrap();

    assert_eq!(before[0].id, after[0].id);
    assert_eq!(before[0].event_type, after[0].event_type);
    assert!(matches!(before[0].status, WebhookStatus::Retrying));
    assert_eq!(before[0].last_http_status, Some(429));
    assert!(matches!(after[0].status, WebhookStatus::Delivered));
    assert_eq!(after[0].attempt_count, 2);
    assert_eq!(after[0].last_http_status, Some(204));
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

fn fixtures(routes: Vec<(&'static str, &'static str)>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let authorization = [
            ("GET /api/users/me", PRINCIPAL),
            ("GET /api/me/workspace-access", ACCESS),
        ];
        for (target, body) in authorization.into_iter().chain(routes) {
            let (mut stream, _) = listener.accept().unwrap();
            let mut input = BufReader::new(&mut stream);
            let mut line = String::new();
            input.read_line(&mut line).unwrap();
            assert_eq!(line.trim_end(), format!("{target} HTTP/1.1"));
            let headers: Vec<_> = input
                .lines()
                .map(Result::unwrap)
                .take_while(|line| !line.is_empty())
                .collect();
            assert!(
                headers
                    .iter()
                    .any(|line| line.starts_with("authorization: Bearer "))
            );
            assert!(!headers.iter().any(|line| line.starts_with("cookie:")));
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (origin, worker)
}
