use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use tempfile::tempdir;
use url::Url;

use super::{
    model::{AccessFilters, GrantSourceKind, GroupDraft},
    service::GovernanceService,
};
use crate::{
    common::error::AppError,
    connection::shared_service::{
        model::{InteractiveAction, ServiceDraft},
        service::SharedService,
    },
};

const PRINCIPAL: &str =
    r#"{"id":17,"email":"delegate@example.org","role":"ui_admin","is_active":true}"#;
const INACTIVE: &str =
    r#"{"id":18,"email":"inactive@example.org","role":"ui_admin","is_active":false}"#;
const ACCESS: &str = r#"{"admin":{"available":true,"context_count":1},"browser":{"available":true,"context_count":1},"manager":{"available":true,"context_count":1},"portal":{"available":false,"context_count":0},"storage_ops":{"available":false,"context_count":0},"ceph_admin":{"available":false,"context_count":0}}"#;
const GROUP: &str = r#"{"id":31,"name":"Operators","description":null,"system_key":null,"user_details":[],"account_details":[],"s3_user_details":[],"s3_connection_details":[],"openstack_cloud_details":[]}"#;
const INHERITED: &str = r#"{"page":2,"total":26,"has_next":false,"items":[{"key":"account-19","scope":"rgw_account","principal":{"id":7,"full_name":"Researcher","email":"reader@example.org","role":"ui_user","is_active":false},"target":{"id":19,"name":"Research","identifier":null},"rights":[{"code":"manager_browser_data_access","sources":[{"kind":"group","group_name":"Operators","group_id":31}]}]}]}"#;

#[tokio::test]
async fn platform_governance_reads_scoped_access_and_exports_all_filtered_rows() {
    let (origin, worker) = serve(vec![
        Fixture::json("GET /api/users/me", 200, PRINCIPAL),
        Fixture::json("GET /api/me/workspace-access", 200, ACCESS),
        Fixture::json(
            "GET /api/admin/access-audit?page=2&search=Research&group_id=31",
            200,
            INHERITED,
        ),
        Fixture {
            target: "GET /api/admin/access-audit/export.csv?search=Research&group_id=31",
            status: 200,
            body: "name,source\r\nResearch,Operators\r\nStage,Operators\r\n",
            content_type: "text/csv; charset=utf-8",
            token: true,
            payload: None,
        },
        Fixture::json(
            "GET /api/admin/access-audit?page=2&search=Research&group_id=31",
            403,
            "{}",
        ),
    ]);
    let directory = tempdir().unwrap();
    let shared = SharedService::open(directory.path().into()).unwrap();
    let profile = shared
        .save(
            None,
            ServiceDraft {
                name: "Research".into(),
                origin,
            },
        )
        .unwrap();
    shared.connect(profile.id, token()).await.unwrap();
    let service = GovernanceService::new(&shared, profile.id);
    let filters = AccessFilters {
        search: Some("Research".into()),
        group_id: Some(31),
        ..Default::default()
    };
    let result = service.access(2, &filters).await.unwrap();
    assert!(!result.items[0].principal.is_active);
    assert!(matches!(
        result.items[0].rights[0].sources[0].kind,
        GrantSourceKind::Group
    ));
    let csv = service.export_access(&filters).await.unwrap();
    assert_eq!(
        csv,
        "name,source\r\nResearch,Operators\r\nStage,Operators\r\n"
    );
    assert!(matches!(
        service.access(2, &filters).await,
        Err(AppError::AuthorizationDenied(_))
    ));
    worker.join().unwrap();
}

#[tokio::test]
async fn platform_governance_uses_browser_for_protected_grants_and_rejects_inactive_auth() {
    let (origin, worker) = serve(vec![
        Fixture::json("GET /api/users/me", 200, INACTIVE),
        Fixture::json("GET /api/users/me", 200, PRINCIPAL),
        Fixture::json("GET /api/me/workspace-access", 200, ACCESS),
        Fixture {
            payload: Some(r#"{"name":"Operators","description":null}"#),
            ..Fixture::json("POST /api/admin/groups", 201, GROUP)
        },
        Fixture {
            token: false,
            ..Fixture::json(
                "GET /api/settings/runtime-surfaces",
                200,
                r#"{"admin":true}"#,
            )
        },
        Fixture {
            token: false,
            ..Fixture::json(
                "GET /api/settings/runtime-surfaces",
                200,
                r#"{"admin":false}"#,
            )
        },
    ]);
    let directory = tempdir().unwrap();
    let shared = SharedService::open(directory.path().into()).unwrap();
    let profile = shared
        .save(
            None,
            ServiceDraft {
                name: "Research".into(),
                origin: origin.clone(),
            },
        )
        .unwrap();
    assert!(matches!(
        shared.connect(profile.id, token()).await,
        Err(AppError::AuthorizationDenied(_))
    ));
    assert!(shared.list().unwrap()[0].session.is_none());
    shared.connect(profile.id, token()).await.unwrap();
    let service = GovernanceService::new(&shared, profile.id);
    let group = service
        .save_group(
            None,
            GroupDraft {
                name: "  Operators ".into(),
                description: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(group.id, 31);

    let review = InteractiveAction::ReviewGroup {
        id: group.id,
        search: "Operators & east/#".into(),
    };
    let url = shared.interactive_url(profile.id, review).await.unwrap();
    assert_eq!(url.origin(), Url::parse(&origin).unwrap().origin());
    assert_eq!(url.path(), "/admin/groups");
    assert_eq!(
        url.query_pairs().collect::<Vec<_>>(),
        [
            ("edit".into(), "31".into()),
            ("search".into(), "Operators & east/#".into())
        ]
    );
    assert!(matches!(
        shared
            .interactive_url(profile.id, InteractiveAction::AuthenticationSettings)
            .await,
        Err(AppError::AuthorizationDenied(_))
    ));
    worker.join().unwrap();
}

#[tokio::test]
async fn platform_governance_accepts_the_dedicated_admin_runtime() {
    let (origin, worker) = serve(vec![
        Fixture::json("GET /api/users/me", 200, PRINCIPAL),
        Fixture::json("GET /api/me/workspace-access", 404, "{}"),
        Fixture {
            token: false,
            ..Fixture::json(
                "GET /api/settings/runtime-surfaces",
                200,
                r#"{"admin":true,"ceph_admin":true,"storage_ops":true,"manager":false,"browser":false,"portal":false}"#,
            )
        },
        Fixture::json("GET /api/admin/access-audit?page=1", 200, INHERITED),
    ]);
    let directory = tempdir().unwrap();
    let shared = SharedService::open(directory.path().into()).unwrap();
    let profile = shared
        .save(
            None,
            ServiceDraft {
                name: "Admin".into(),
                origin,
            },
        )
        .unwrap();
    let session = shared.connect(profile.id, token()).await.unwrap();
    assert!(session.admin_available());
    assert!(!session.access.manager.available);
    assert!(!session.access.browser.available);
    let service = GovernanceService::new(&shared, profile.id);
    assert!(
        !service
            .access(1, &AccessFilters::default())
            .await
            .unwrap()
            .items
            .is_empty()
    );
    worker.join().unwrap();
}

struct Fixture {
    target: &'static str,
    status: u16,
    body: &'static str,
    content_type: &'static str,
    token: bool,
    payload: Option<&'static str>,
}

impl Fixture {
    fn json(target: &'static str, status: u16, body: &'static str) -> Self {
        Self {
            target,
            status,
            body,
            content_type: "application/json",
            token: true,
            payload: None,
        }
    }
}

fn token() -> String {
    let expiry = (Utc::now() + Duration::hours(1)).timestamp();
    let claims = URL_SAFE_NO_PAD.encode(json!({"typ": "api_access", "exp": expiry}).to_string());
    format!("e30.{claims}.governance-fixture")
}

fn serve(fixtures: Vec<Fixture>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        for fixture in fixtures {
            let (mut stream, _) = listener.accept().unwrap();
            check_request(&mut stream, &fixture);
            write!(stream, "HTTP/1.1 {} Fixture\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", fixture.status, fixture.content_type, fixture.body.len(), fixture.body).unwrap();
        }
    });
    (origin, worker)
}

fn check_request(stream: &mut TcpStream, fixture: &Fixture) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line.trim_end(), format!("{} HTTP/1.1", fixture.target));
    let mut payload_length = 0;
    let mut authorized = false;
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line.trim().is_empty() {
            break;
        }
        let (header, value) = line.split_once(':').unwrap();
        match header.to_ascii_lowercase().as_str() {
            "content-length" => payload_length = value.trim().parse().unwrap(),
            "authorization" => {
                assert!(value.trim().starts_with("Bearer "));
                authorized = true;
            }
            "cookie" => panic!("Service API requests cannot include browser cookies"),
            _ => {}
        }
    }
    assert_eq!(authorized, fixture.token);
    if let Some(expected) = fixture.payload {
        let mut payload = vec![0; payload_length];
        reader.read_exact(&mut payload).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&payload).unwrap(),
            serde_json::from_str::<Value>(expected).unwrap()
        );
    }
}
