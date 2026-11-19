use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    thread,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde_json::json;
use tempfile::tempdir;

use super::{
    model::{PortalChange, PortalRead},
    service,
};
use crate::{
    common::error::AppError,
    connection::shared_service::{model::ServiceDraft, service::SharedService},
};

const PRINCIPAL: &str =
    r#"{"id":17,"email":"portal@example.org","role":"ui_user","is_active":true}"#;
const ACCESS: &str = r#"{"admin":{"available":false,"context_count":0},"browser":{"available":false,"context_count":0},"manager":{"available":false,"context_count":0},"portal":{"available":true,"context_count":2},"storage_ops":{"available":false,"context_count":0},"ceph_admin":{"available":false,"context_count":0}}"#;

#[tokio::test]
async fn scopes_files_to_published_bucket_and_rechecks_writes_after_archive() {
    let directory = tempdir().unwrap();
    let space = |role, status| {
        json!({
            "name": "Working files", "role": role, "status": status,
            "can_browse": true, "can_delete": false, "internal_bucket_name": "effective-bucket"
        })
        .to_string()
    };
    let (origin, token, worker) = serve(vec![
        Request::new("GET", "/api/portal/accounts", 200, r#"[{"id":12},{"id":34}]"#),
        Request::new("GET", "/api/portal/storage-spaces/docs?account_id=12", 200, space("Viewer", "Active")),
        Request::new("GET", "/api/browser/buckets/effective-bucket/objects?context_id=12&prefix=drafts%2F&max_keys=100", 200, r#"{"objects":[{"key":"drafts/shared.txt"}]}"#).browser(),
        Request::new("GET", "/api/portal/storage-spaces/docs?account_id=12", 200, space("Viewer", "Active")),
        Request::new("GET", "/api/portal/storage-spaces/docs?account_id=34", 200, space("Editor", "Archived")),
        Request::new("PATCH", "/api/portal/storage-spaces/docs?account_id=34", 200, space("Manager", "Active")).body(json!({"archived":false})),
        Request::new("GET", "/api/portal/storage-spaces/docs?account_id=34", 200, space("Editor", "Active")),
        Request::new("POST", "/api/browser/buckets/effective-bucket/folders?context_id=34", 204, "").browser().body(json!({"prefix":"reopened/"})),
        Request::new("PUT", "/api/portal/storage-spaces/docs/settings?account_id=34", 200, r#"{"versioning_enabled":true}"#)
            .body(json!({"versioning_enabled":true,"lifecycle_enabled":true,"version_history_retention_days":90})),
        Request::new("GET", "/api/portal/storage-spaces/docs/objects/versions?account_id=34&max_keys=100&key=drafts%2Fshared.txt", 200, r#"{"versions":[{"version_id":"historical"}]}"#),
    ]);
    let shared = SharedService::open(directory.path().into()).unwrap();
    let profile = shared
        .save(
            None,
            ServiceDraft {
                name: "Portal".into(),
                origin,
            },
        )
        .unwrap();
    shared.connect(profile.id, token).await.unwrap();
    let projects = service::read(&shared, profile.id, None, None, PortalRead::Projects)
        .await
        .unwrap();
    assert_eq!(projects, json!([{"id":12},{"id":34}]));
    let space_id = || Some("docs".into());
    let files = service::read(
        &shared,
        profile.id,
        Some(12),
        space_id(),
        PortalRead::Files {
            prefix: "drafts/".into(),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(files["objects"][0]["key"], "drafts/shared.txt");
    for project in [12, 34] {
        let change = PortalChange::Folder {
            prefix: "reopened/".into(),
        };
        assert!(matches!(
            service::change(&shared, profile.id, project, space_id(), change).await,
            Err(AppError::AuthorizationDenied(_))
        ));
    }
    service::change(
        &shared,
        profile.id,
        34,
        space_id(),
        PortalChange::Archive { archived: false },
    )
    .await
    .unwrap();
    service::change(
        &shared,
        profile.id,
        34,
        space_id(),
        PortalChange::Folder {
            prefix: "reopened/".into(),
        },
    )
    .await
    .unwrap();
    let settings = serde_json::from_value(json!({"kind":"spaceSettings","draft":{
        "versioning_enabled":true,"lifecycle_enabled":true,"version_history_retention_days":90
    }}))
    .unwrap();
    service::change(&shared, profile.id, 34, space_id(), settings)
        .await
        .unwrap();
    let versions = service::read(
        &shared,
        profile.id,
        Some(34),
        space_id(),
        PortalRead::Versions {
            key: "drafts/shared.txt".into(),
            key_marker: None,
            version_id_marker: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(versions["versions"][0]["version_id"], "historical");
    assert!(matches!(
        service::read(
            &shared,
            profile.id,
            Some(34),
            Some("../other".into()),
            PortalRead::Space
        )
        .await,
        Err(AppError::Invalid(_))
    ));
    worker.join().unwrap();
}

pub(super) struct Request {
    method: &'static str,
    target: &'static str,
    status: u16,
    response: String,
    body: Option<serde_json::Value>,
    browser: bool,
}

impl Request {
    pub(super) fn new(
        method: &'static str,
        target: &'static str,
        status: u16,
        response: impl Into<String>,
    ) -> Self {
        Self {
            method,
            target,
            status,
            response: response.into(),
            body: None,
            browser: false,
        }
    }

    pub(super) fn body(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body);
        self
    }

    fn browser(mut self) -> Self {
        self.browser = true;
        self
    }
}

pub(super) fn serve(requests: Vec<Request>) -> (String, String, thread::JoinHandle<()>) {
    serve_as(requests, false)
}

pub(super) fn serve_as(
    requests: Vec<Request>,
    admin: bool,
) -> (String, String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let claims = json!({"typ":"api_access","exp":Utc::now().timestamp() + 300});
    let token = format!("x.{}.x", URL_SAFE_NO_PAD.encode(claims.to_string()));
    let expected = format!("authorization: Bearer {token}");
    let worker = thread::spawn(move || {
        let requests = [
            Request::new(
                "GET",
                "/api/users/me",
                200,
                if admin {
                    PRINCIPAL.replace("ui_user", "ui_admin")
                } else {
                    PRINCIPAL.into()
                },
            ),
            Request::new(
                "GET",
                "/api/me/workspace-access",
                200,
                if admin {
                    ACCESS.replace(
                        r#""admin":{"available":false,"context_count":0}"#,
                        r#""admin":{"available":true,"context_count":1}"#,
                    )
                } else {
                    ACCESS.into()
                },
            ),
        ]
        .into_iter()
        .chain(requests);
        for request in requests {
            let (mut socket, _) = listener.accept().unwrap();
            let mut input = BufReader::new(&mut socket);
            let mut line = String::new();
            input.read_line(&mut line).unwrap();
            assert_eq!(
                line.trim_end(),
                format!("{} {} HTTP/1.1", request.method, request.target)
            );
            let mut headers = Vec::new();
            loop {
                line.clear();
                input.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                headers.push(line.trim_end().to_string());
            }
            assert!(headers.contains(&expected));
            assert_eq!(
                headers
                    .iter()
                    .any(|header| header.eq_ignore_ascii_case("x-s3-workspace: portal")),
                request.browser
            );
            if let Some(body) = request.body {
                let length = headers
                    .iter()
                    .find_map(|header| header.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                let mut bytes = vec![0; length];
                input.read_exact(&mut bytes).unwrap();
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                    body
                );
            }
            write!(socket, "HTTP/1.1 {} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                request.status, request.response.len(), request.response).unwrap();
        }
    });
    (origin, token, worker)
}
