use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicU8, Ordering},
};

use aws_credential_types::provider::ProvideCredentials;
use chrono::Duration as ChronoDuration;
use serde_json::{Value, json};

use crate::connection::{
    model::{ConnectionDraft, EndpointSettings, IdentityKind, Secret},
    repository::CredentialRepository,
    service::ConnectionService,
};

use super::*;

struct MockKeystone {
    address: std::net::SocketAddr,
    scenario: Arc<AtomicU8>,
    requests: Arc<Mutex<Vec<(String, String)>>>,
}

impl MockKeystone {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let scenario = Arc::new(AtomicU8::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (server_scenario, server_requests) = (Arc::clone(&scenario), Arc::clone(&requests));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let stream = stream.unwrap();
                if !serve(stream, &server_scenario, &server_requests) {
                    break;
                }
            }
        });
        Self {
            address,
            scenario,
            requests,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/keystone/v3", self.address)
    }

    fn change(&self, scenario: u8) {
        self.scenario.store(scenario, Ordering::Relaxed);
    }
}

impl Drop for MockKeystone {
    fn drop(&mut self) {
        drop(TcpStream::connect(self.address));
    }
}

fn serve(
    mut stream: TcpStream,
    scenario: &AtomicU8,
    requests: &Mutex<Vec<(String, String)>>,
) -> bool {
    let mut request = BufReader::new(&mut stream);
    let mut line = String::new();
    if request.read_line(&mut line).unwrap() == 0 {
        return false;
    }
    let parts = line.split_whitespace().collect::<Vec<_>>();
    let (method, path) = (parts[0].to_owned(), parts[1].to_owned());
    requests
        .lock()
        .unwrap()
        .push((method.clone(), path.clone()));
    let mut headers = BTreeMap::new();
    loop {
        line.clear();
        request.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        let (key, value) = line.split_once(':').unwrap();
        headers.insert(key.to_ascii_lowercase(), value.trim().to_owned());
    }
    let size = headers
        .get("content-length")
        .map_or(0, |size| size.parse().unwrap());
    let mut body = vec![0; size];
    request.read_exact(&mut body).unwrap();
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    let (status, token, payload) = keystone_response(
        &method,
        &path,
        &headers,
        &body,
        scenario.load(Ordering::Relaxed),
    );
    let payload = payload.to_string();
    let token = token.map_or(String::new(), |value| {
        format!("X-Subject-Token: {value}\r\n")
    });
    write!(stream, "HTTP/1.1 {status}\r\n{token}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len()).unwrap();
    true
}

fn keystone_response(
    method: &str,
    path: &str,
    headers: &BTreeMap<String, String>,
    body: &Value,
    scenario: u8,
) -> (&'static str, Option<&'static str>, Value) {
    let path = path.strip_prefix("/keystone/v3").unwrap();
    match (method, path) {
        ("GET", "/auth/tokens") => {
            assert_eq!(headers["x-auth-token"], "primary-token");
            assert_eq!(headers["x-subject-token"], "primary-token");
            let mut token = token_document(None, scenario);
            if scenario == 3 {
                token["token"]["OS-TRUST:trust"] = json!({"id": "delegation"});
            }
            ("200 OK", None, token)
        }
        ("POST", "/auth/tokens") => {
            let project = body
                .pointer("/auth/scope/project/id")
                .and_then(Value::as_str);
            let identity = &body["auth"]["identity"];
            if project.is_none() {
                assert_eq!(body["auth"]["scope"], "unscoped");
                assert!(
                    identity["password"]["user"]["domain"]["name"] == "Default"
                        || identity["token"]["id"] == "primary-token"
                );
            } else {
                assert_eq!(identity["token"]["id"], "unscoped-token");
            }
            (
                "201 Created",
                Some(if project.is_some() {
                    "scoped-token"
                } else {
                    "unscoped-token"
                }),
                token_document(project, scenario),
            )
        }
        ("GET", "/auth/projects") => {
            assert_eq!(headers["x-auth-token"], "unscoped-token");
            (
                "200 OK",
                None,
                json!({"projects": [
                {"id": "project-a", "name": "Research", "domain_id": "default"},
                {"id": "disabled", "name": "Disabled", "enabled": false}
            ], "links": {"next": "?page=2"}}),
            )
        }
        ("GET", "/auth/projects?page=2") => (
            "200 OK",
            None,
            json!({"projects": [{"id": "project-b", "name": "Build"}]}),
        ),
        ("GET", "/users/operator/credentials/OS-EC2") => {
            assert_eq!(headers["x-auth-token"], "scoped-token");
            (
                "200 OK",
                None,
                json!({"credentials": [
                    {"user_id": "operator", "tenant_id": "project-a", "access": "key-one", "secret": "secret-one"},
                    {"user_id": "operator", "project_id": "project-a", "access": "key-two", "secret": "secret-two"},
                    {"user_id": "operator", "project_id": "project-b", "access": "foreign-key", "secret": "foreign-secret"}
                ]}),
            )
        }
        _ => ("404 Not Found", None, json!({})),
    }
}

fn token_document(project: Option<&str>, scenario: u8) -> Value {
    let expired = scenario == 2;
    let expires = Utc::now() + ChronoDuration::minutes(if expired { -1 } else { 15 });
    let mut document = json!({"token": {
        "user": {"id": "operator", "name": "Keystone Operator"},
        "expires_at": expires, "methods": ["password"]
    }});
    if let Some(id) = project {
        document["token"]["project"] = json!({"id": id});
        document["token"]["catalog"] = catalog(scenario == 1);
    }
    document
}

fn catalog(changed: bool) -> Value {
    json!([
        {"type": "s3", "endpoints": [
            {"id": "rome-public", "region_id": "Rome", "interface": "public",
                "url": if changed { "https://relocated.example.org/s3" } else { "https://rome.example.org/s3" }},
            {"id": "rome-private", "region_id": "Rome", "interface": "internal", "url": "https://internal.example.org"},
            {"id": "paris", "region": "Paris", "interface": "public", "url": "https://paris.example.org"},
            {"id": "swift-tagged-s3", "region_id": "Swift", "interface": "public", "url": "https://swift.example.org/%76%31/AUTH_Project"}
        ]},
        {"type": "object-store", "endpoints": [
            {"id": "swift", "region_id": "Swift", "interface": "public", "url": "https://swift.example.org/swift/v1/AUTH_Project"}
        ]}
    ])
}

struct MemoryCredentials(Mutex<HashMap<Uuid, Secret>>);

impl CredentialRepository for MemoryCredentials {
    fn read(&self, id: Uuid) -> Result<Secret> {
        self.0
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::Credential("Missing test credential".into()))
    }

    fn write(&self, id: Uuid, secret: &Secret) -> Result<()> {
        self.0.lock().unwrap().insert(id, secret.clone());
        Ok(())
    }

    fn remove(&self, id: Uuid) {
        self.0.lock().unwrap().remove(&id);
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    keystone: MockKeystone,
    connections: ConnectionService,
    cloud: Cloud,
}

impl Fixture {
    fn new(managed: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let keystone = MockKeystone::start();
        let repo = ConnectionRepository::open(directory.path().to_path_buf()).unwrap();
        let connections =
            ConnectionService::with_credentials(repo, MemoryCredentials(Mutex::default()));
        let cloud = connections
            .openstack
            .save(
                None,
                CloudSettings {
                    name: "Research Cloud".into(),
                    auth_url: keystone.url(),
                    default_domain: "Default".into(),
                    managed_endpoints_only: managed,
                },
            )
            .unwrap();
        Self {
            directory,
            keystone,
            connections,
            cloud,
        }
    }

    async fn sign_in(&self) -> Result<SessionView> {
        self.connections
            .openstack
            .authenticate(
                self.cloud.id,
                Authentication::Password {
                    username: "operator".into(),
                    domain: String::new(),
                    password: "password".into(),
                },
            )
            .await
    }

    async fn discover(&self) -> Result<ProjectAccess> {
        self.connections
            .openstack
            .discover(self.cloud.id, "project-a".into())
            .await
    }

    async fn activate(
        &self,
        access: &ProjectAccess,
        target: &str,
        key: &str,
    ) -> Result<Connection> {
        self.connections
            .openstack
            .activate(
                self.cloud.id,
                Activation {
                    discovery_id: access.id,
                    target_id: target.into(),
                    access_key: key.into(),
                },
            )
            .await
    }
}

#[tokio::test]
async fn openstack_context_binds_selected_target_and_key_to_both_workspaces() {
    let fixture = Fixture::new(false);
    let session = fixture.sign_in().await.unwrap();
    assert_eq!(
        session
            .projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>(),
        ["project-b", "project-a"]
    );
    let access = fixture.discover().await.unwrap();
    assert_eq!(
        access
            .targets
            .iter()
            .map(|target| target.catalog.id.as_str())
            .collect::<Vec<_>>(),
        ["paris", "rome-private", "rome-public"]
    );
    assert_eq!(access.access_keys, ["key-one", "key-two"]);
    assert!(fixture.activate(&access, "", "").await.is_err());
    assert!(
        fixture
            .activate(&access, "paris", "foreign-key")
            .await
            .is_err()
    );

    let profile = fixture
        .activate(&access, "rome-private", "key-two")
        .await
        .unwrap();
    assert_eq!(profile.endpoint, "https://internal.example.org");
    assert_eq!(profile.region, "Rome");
    for workspace in [Workspace::Browser, Workspace::Manager] {
        let session = fixture
            .connections
            .session_for(profile.id, workspace, IdentityKind::S3)
            .unwrap();
        assert_eq!(session.context.account, "project-a");
        assert_eq!(session.profile.endpoint, profile.endpoint);
        assert_eq!(session.credentials.access_key_id(), "key-two");
        session.validate().await.unwrap();
    }
    assert!(
        fixture
            .connections
            .session_for(profile.id, Workspace::CephAdmin, IdentityKind::AdminOps)
            .is_err()
    );
    assert_eq!(
        fixture
            .activate(&access, "rome-private", "key-two")
            .await
            .unwrap()
            .id,
        profile.id
    );
}

#[tokio::test]
async fn openstack_context_inherits_managed_features_and_denies_unregistered_regions() {
    let fixture = Fixture::new(true);
    let direct = fixture
        .connections
        .save(ConnectionDraft {
            id: None,
            name: "Registered Rome".into(),
            provider: Provider::Ceph,
            endpoint: "https://rome.example.org/s3/".into(),
            region: "Rome".into(),
            path_style: false,
            known_buckets: vec!["personal-bucket".into()],
            credentials: Some(Secret {
                access_key: "local-direct".into(),
                secret_key: "local-secret".into(),
                session_token: None,
            }),
            settings: Some(EndpointSettings {
                browser_enabled: false,
                standard_tags: vec!["Rome".into()],
                ..EndpointSettings::default()
            }),
            validate_s3: false,
            reconcile: BTreeMap::new(),
        })
        .await
        .unwrap();
    fixture.sign_in().await.unwrap();
    let access = fixture.discover().await.unwrap();
    assert_eq!(
        access
            .targets
            .iter()
            .filter(|region| region.allowed)
            .map(|region| region.catalog.id.as_str())
            .collect::<Vec<_>>(),
        ["rome-public"]
    );
    assert!(fixture.activate(&access, "paris", "key-one").await.is_err());

    let project = fixture
        .activate(&access, "rome-public", "key-one")
        .await
        .unwrap();
    assert_eq!(project.endpoint_id(), direct.endpoint_id());
    assert!(!project.path_style);
    assert_eq!(project.settings.standard_tags, ["Rome"]);
    assert!(project.services.is_empty() && project.known_buckets.is_empty());
    assert!(fixture.connections.session(project.id).is_err());
    assert!(
        fixture
            .connections
            .session_for(project.id, Workspace::Manager, IdentityKind::S3)
            .is_ok()
    );
    assert!(fixture.connections.bucket_session(project.id).is_ok());
    fixture.connections.remove(direct.id).unwrap();
    assert!(matches!(
        fixture
            .connections
            .session_for(project.id, Workspace::Manager, IdentityKind::S3),
        Err(AppError::Reauthenticate(_))
    ));
}

#[tokio::test]
async fn openstack_context_requires_sign_in_on_catalog_change_before_and_after_activation() {
    let fixture = Fixture::new(false);
    fixture.sign_in().await.unwrap();
    let access = fixture.discover().await.unwrap();
    fixture.keystone.change(1);
    assert!(matches!(
        fixture.activate(&access, "rome-public", "key-one").await,
        Err(AppError::Reauthenticate(_))
    ));
    assert!(fixture.connections.openstack.list().unwrap()[0].reconnect_required);

    fixture.sign_in().await.unwrap();
    let access = fixture.discover().await.unwrap();
    let profile = fixture
        .activate(&access, "rome-public", "key-one")
        .await
        .unwrap();
    assert_eq!(profile.endpoint, "https://relocated.example.org/s3");
    let lease = fixture
        .connections
        .openstack
        .lease(profile.id)
        .unwrap()
        .unwrap();
    let provider = ProjectCredentials {
        credentials: lease.credentials.clone(),
        lease: Arc::clone(&lease),
    };
    fixture.keystone.change(0);
    lease.validation.lock().await.checked_at = Instant::now() - REVALIDATION_INTERVAL;
    assert!(provider.provide_credentials().await.is_err());
    assert!(fixture.connections.list().unwrap().is_empty());
    assert!(fixture.connections.openstack.list().unwrap()[0].reconnect_required);
}

#[tokio::test]
async fn openstack_context_rejects_delegation_and_expiry() {
    let fixture = Fixture::new(false);
    let authenticate = || {
        fixture.connections.openstack.authenticate(
            fixture.cloud.id,
            Authentication::Token {
                token: "primary-token".into(),
            },
        )
    };
    fixture.keystone.change(3);
    assert!(authenticate().await.is_err());
    fixture.keystone.change(2);
    assert!(matches!(
        authenticate().await,
        Err(AppError::Reauthenticate(_))
    ));
    fixture.keystone.change(0);
    authenticate().await.unwrap();
    let access = fixture.discover().await.unwrap();
    let profile = fixture.activate(&access, "paris", "key-one").await.unwrap();
    let lease = fixture
        .connections
        .openstack
        .lease(profile.id)
        .unwrap()
        .unwrap();
    lease.validation.lock().await.expires_at = Utc::now() - ChronoDuration::seconds(1);
    let error = lease.validate().await.unwrap_err();
    assert!(matches!(error, AppError::Reauthenticate(_)));
    assert!(fixture.connections.list().unwrap().is_empty());
    assert!(fixture.connections.openstack.list().unwrap()[0].reconnect_required);
}

#[tokio::test]
async fn openstack_context_disconnect_and_forget_are_local() {
    let fixture = Fixture::new(false);
    fixture.sign_in().await.unwrap();
    let access = fixture.discover().await.unwrap();
    let profile = fixture.activate(&access, "paris", "key-one").await.unwrap();
    let held_session = fixture.connections.session(profile.id).unwrap();
    let presign = || {
        held_session
            .s3
            .get_object()
            .bucket("objects")
            .key("note.txt")
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(Duration::from_secs(60))
                    .unwrap(),
            )
    };
    let presigned = presign().await.unwrap();
    let signed_url = url::Url::parse(presigned.uri()).unwrap();
    assert_eq!(signed_url.host_str(), Some("paris.example.org"));
    assert!(
        signed_url
            .query_pairs()
            .any(|(name, value)| name == "X-Amz-Credential" && value.starts_with("key-one/"))
    );
    let definitions = fixture.directory.path().join("openstack-clouds.json");
    let saved = std::fs::read_to_string(&definitions).unwrap();
    assert!(saved.contains("Research Cloud"));
    assert!(!saved.contains("unscoped-token") && !saved.contains("secret-one"));
    let requests = fixture.keystone.requests.lock().unwrap().len();

    fixture
        .connections
        .openstack
        .disconnect(fixture.cloud.id, false)
        .unwrap();
    assert!(held_session.validate().await.is_err());
    assert!(presign().await.is_err());
    assert!(fixture.connections.list().unwrap().is_empty());
    assert!(
        fixture.connections.openstack.list().unwrap()[0]
            .session
            .is_none()
    );
    fixture
        .connections
        .openstack
        .disconnect(fixture.cloud.id, true)
        .unwrap();
    assert!(fixture.connections.openstack.list().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(definitions).unwrap(), "[]");
    assert_eq!(fixture.keystone.requests.lock().unwrap().len(), requests);
}
