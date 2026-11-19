use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    thread,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use serde_json::json;
use tempfile::tempdir;

use super::{
    model::{InteractiveAction, ServiceDraft, SharedWorkspace},
    service::SharedService,
};
use crate::common::error::AppError;

const PRINCIPAL: &str =
    r#"{"id":17,"email":"delegate@example.org","role":"ui_admin","is_active":true}"#;
const ACCESS: &str = r#"{"admin":{"available":true,"context_count":1},"browser":{"available":true,"context_count":1},"manager":{"available":true,"context_count":1},"portal":{"available":false,"context_count":0},"storage_ops":{"available":false,"context_count":0},"ceph_admin":{"available":false,"context_count":0}}"#;
const CONTEXTS: &str = r#"[{"id":"account:18","kind":"account","display_name":"Research","connection_scope":null,"endpoint_name":"Rome","endpoint_url":"https://s3.example.org","manager_role":"account_administrator","portal_role":null,"capabilities":{"can_manage_iam":true,"sts_capable":false,"admin_api_capable":false}}]"#;
const SURFACES: &str = r#"{"admin":true}"#;

#[tokio::test]
async fn shared_service_binds_scope_denial_expiry_and_browser_origin() {
    let directory = tempdir().unwrap();
    let token = test_token();
    let (origin, worker) = serve(
        token.clone(),
        vec![
            ("/api/users/me", 200, PRINCIPAL, true),
            ("/api/me/workspace-access", 200, ACCESS, true),
            (
                "/api/me/execution-contexts?workspace=manager",
                200,
                CONTEXTS,
                true,
            ),
            (
                "/api/me/execution-contexts?workspace=browser",
                403,
                "{}",
                true,
            ),
            ("/api/settings/runtime-surfaces", 200, SURFACES, false),
            (
                "/api/me/execution-contexts?workspace=manager",
                401,
                "{}",
                true,
            ),
        ],
    );
    let service = SharedService::open(directory.path().into()).unwrap();
    let profile = service
        .save(
            None,
            ServiceDraft {
                name: "Research".into(),
                origin: origin.clone(),
            },
        )
        .unwrap();
    let session = service.connect(profile.id, token).await.unwrap();
    assert_eq!(session.principal.email, "delegate@example.org");
    assert!(session.access.manager.available);
    assert!(!session.access.portal.available);
    assert_eq!(
        service
            .contexts(profile.id, SharedWorkspace::Manager)
            .await
            .unwrap()[0]
            .name,
        "Research"
    );
    assert!(matches!(
        service.contexts(profile.id, SharedWorkspace::Browser).await,
        Err(AppError::AuthorizationDenied(_))
    ));
    assert!(service.list().unwrap()[0].session.is_some());
    let url = service
        .interactive_url(profile.id, InteractiveAction::IdentitySecurity)
        .await
        .unwrap();
    assert_eq!(url.as_str(), format!("{origin}/admin/identity-security"));
    assert!(matches!(
        service.contexts(profile.id, SharedWorkspace::Manager).await,
        Err(AppError::Reauthenticate(_))
    ));
    assert!(service.list().unwrap()[0].session.is_none());
    service.disconnect(profile.id, true).unwrap();
    assert!(service.list().unwrap().is_empty());
    worker.join().unwrap();
}

#[test]
fn shared_service_rejects_non_origin_urls() {
    let directory = tempdir().unwrap();
    let service = SharedService::open(directory.path().into()).unwrap();
    for origin in [
        "http://remote.example.org",
        "https://user@remote.example.org",
        "https://remote.example.org/api",
        "https://remote.example.org/?redirect=https://other.example.org",
        "file:///tmp",
    ] {
        assert!(matches!(
            service.save(
                None,
                ServiceDraft {
                    name: "Invalid".into(),
                    origin: origin.into()
                }
            ),
            Err(AppError::Invalid(_))
        ));
    }
}

fn test_token() -> String {
    let claims = json!({"typ": "api_access", "exp": (Utc::now() + Duration::hours(1)).timestamp()});
    format!("e30.{}.fixture", URL_SAFE_NO_PAD.encode(claims.to_string()))
}

fn serve(
    token: String,
    responses: Vec<(&'static str, u16, &'static str, bool)>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        for (route, status, body, authenticated) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            check_request(&mut stream, route, authenticated.then_some(token.as_str()));
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nSet-Cookie: session=fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (origin, worker)
}

fn check_request(stream: &mut TcpStream, route: &str, token: Option<&str>) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line.trim_end(), format!("GET {route} HTTP/1.1"));
    let mut authorization = None;
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line.trim().is_empty() {
            break;
        }
        let (header, value) = line.split_once(':').unwrap();
        if header.eq_ignore_ascii_case("authorization") {
            authorization = Some(value.trim().to_owned());
        }
        assert!(!header.eq_ignore_ascii_case("cookie"));
    }
    assert_eq!(authorization, token.map(|value| format!("Bearer {value}")));
}
