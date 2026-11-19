use super::*;

#[test]
fn rejects_credentials_in_endpoint_urls() {
    assert!(validate_endpoint("https://key:secret@storage.example.test", Provider::Ceph).is_err());
    assert!(
        validate_endpoint("https://storage.example.test?token=secret", Provider::Ceph).is_err()
    );
}

#[test]
fn preserves_custom_endpoint_prefix() {
    assert_eq!(
        validate_endpoint("https://storage.example.test/s3/", Provider::Ceph).unwrap(),
        "https://storage.example.test/s3",
    );
    assert_eq!(validate_endpoint("", Provider::Aws).unwrap(), "");
}

#[test]
fn execution_context_preserves_legacy_profile_and_keyring_reference() {
    let stored: StoredConnection = serde_json::from_value(serde_json::json!({
        "profile": {
            "id": "83abaf65-8aa9-496f-81dc-dd1dae21025a",
            "name": "Legacy",
            "provider": "ceph",
            "endpoint": "https://storage.example.test",
            "region": "us-east-1",
            "pathStyle": true,
            "knownBuckets": ["shared"]
        },
        "secret_id": "dd307edf-cb37-4494-a029-a9d472ad6c13"
    }))
    .unwrap();

    assert_eq!(stored.profile.endpoint_id(), stored.profile.id);
    assert_eq!(
        stored.profile.principal_id(IdentityKind::S3),
        Some(stored.profile.id)
    );
    assert_eq!(stored.profile.known_buckets, ["shared"]);
    assert!(stored.profile.path_style);
    assert!(stored.profile.settings.browser_enabled);
    assert!(stored.profile.settings.manager_enabled);
    assert_eq!(
        stored.secret_id.to_string(),
        "dd307edf-cb37-4494-a029-a9d472ad6c13"
    );
    assert!(identity_available(&stored.profile, IdentityKind::S3).is_ok());
    assert!(identity_available(&stored.profile, IdentityKind::CephAdmin).is_err());
}

#[derive(Default)]
struct MockKeyring(std::sync::Mutex<BTreeMap<Uuid, Secret>>);

impl CredentialRepository for MockKeyring {
    fn read(&self, id: Uuid) -> Result<Secret> {
        self.0
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::Credential("Missing test credential.".into()))
    }

    fn write(&self, id: Uuid, secret: &Secret) -> Result<()> {
        self.0.lock().unwrap().insert(id, secret.clone());
        Ok(())
    }

    fn remove(&self, id: Uuid) {
        self.0.lock().unwrap().remove(&id);
    }
}

fn draft_for(endpoint: &str, tenant: &str, access_key: &str) -> ConnectionDraft {
    ConnectionDraft {
        id: None,
        name: tenant.into(),
        provider: Provider::Ceph,
        endpoint: endpoint.into(),
        region: "us-east-1".into(),
        path_style: true,
        known_buckets: vec!["archive".into()],
        credentials: Some(fake_secret(access_key)),
        settings: Some(super::super::model::EndpointSettings {
            tenant: tenant.into(),
            ..Default::default()
        }),
        validate_s3: false,
        reconcile: BTreeMap::new(),
    }
}

fn fake_secret(access_key: &str) -> Secret {
    Secret {
        access_key: access_key.into(),
        secret_key: "mock-secret-key".into(),
        session_token: None,
    }
}

fn mock_endpoint(
    responses: Vec<(&'static str, &'static str)>,
) -> (String, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        responses
            .into_iter()
            .map(|(status, body)| {
                let deadline = std::time::Instant::now() + Duration::from_secs(12);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("Mock endpoint did not receive the request: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = String::new();
                let mut reader = BufReader::new(&mut stream);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                request
            })
            .collect()
    });
    (url, worker)
}

#[tokio::test]
async fn execution_context_signs_a_mutation_with_the_selected_tenants_credential() {
    let (endpoint, requests) = mock_endpoint(vec![("200 OK", "")]);
    let directory = tempfile::tempdir().unwrap();
    let service = ConnectionService::with_credentials(
        ConnectionRepository::open(directory.path().into()).unwrap(),
        MockKeyring::default(),
    );
    let first = service
        .save(draft_for(&endpoint, "tenant-a", "ordinary-a"))
        .await
        .unwrap();
    let second = service
        .save(draft_for(&endpoint, "tenant-b", "ordinary-b"))
        .await
        .unwrap();
    let first_session = service.session(first.id).unwrap();
    let selected = service.session(second.id).unwrap();
    assert_eq!(
        first_session.context.endpoint_id,
        selected.context.endpoint_id
    );
    assert_ne!(
        first_session.context.principal_id,
        selected.context.principal_id
    );
    assert_eq!(selected.context.tenant, "tenant-b");

    selected
        .s3
        .put_object()
        .bucket("archive")
        .key("selected/")
        .send()
        .await
        .unwrap();
    let requests = requests.join().unwrap();
    assert!(requests[0].starts_with("PUT /archive/selected/"));
    assert!(requests[0].contains("Credential=ordinary-b/"));
    assert!(!requests[0].contains("ordinary-a"));
}

#[tokio::test]
async fn execution_context_reconciles_only_selected_services_and_guards_denied_identities() {
    let (endpoint, requests) = mock_endpoint(vec![("200 OK", "[]"), ("403 Forbidden", "{}")]);
    let directory = tempfile::tempdir().unwrap();
    let service = ConnectionService::with_credentials(
        ConnectionRepository::open(directory.path().into()).unwrap(),
        MockKeyring::default(),
    );
    let mut draft = draft_for(&endpoint, "tenant-a", "ordinary");
    draft.reconcile.insert(
        IdentityKind::CephAdmin,
        ServiceDraft {
            endpoint: String::new(),
            enabled: true,
            credentials: Some(fake_secret("dedicated")),
        },
    );
    let saved = service.save(draft).await.unwrap();
    let principal = saved.principal_id(IdentityKind::CephAdmin);
    assert_eq!(
        saved.capability(IdentityKind::CephAdmin),
        CapabilityState::Detected
    );

    let mut update = draft_for(&endpoint, "tenant-a", "ordinary");
    update.id = Some(saved.id);
    update.credentials = None;
    update.reconcile.insert(
        IdentityKind::Supervision,
        ServiceDraft {
            endpoint: String::new(),
            enabled: true,
            credentials: Some(fake_secret("supervisor")),
        },
    );
    let updated = service.save(update).await.unwrap();
    assert_eq!(updated.principal_id(IdentityKind::CephAdmin), principal);
    assert_eq!(
        updated.capability(IdentityKind::CephAdmin),
        CapabilityState::Detected
    );
    assert_eq!(
        updated.capability(IdentityKind::Supervision),
        CapabilityState::Denied
    );
    assert_eq!(
        updated.capability(IdentityKind::Runtime),
        CapabilityState::Unavailable
    );

    assert!(
        service
            .session_for(updated.id, Workspace::CephAdmin, IdentityKind::Supervision)
            .is_err()
    );
    let dedicated = service
        .session_for(updated.id, Workspace::CephAdmin, IdentityKind::CephAdmin)
        .unwrap();
    assert_eq!(dedicated.credentials.access_key_id(), "dedicated");
    assert_eq!(
        service
            .session(updated.id)
            .unwrap()
            .credentials
            .access_key_id(),
        "ordinary"
    );
    let requests = requests.join().unwrap();
    assert!(requests[0].starts_with("GET /admin/bucket?"));
    assert!(requests[0].contains("Credential=dedicated/"));
    assert!(requests[1].starts_with("GET /admin/usage?"));
    assert!(requests[1].contains("Credential=supervisor/"));
}

#[test]
fn execution_context_rejects_invalid_ca_and_private_key_input() {
    assert!(validate_ca("bad PEM").is_err());
    assert!(validate_ca("-----BEGIN PRIVATE KEY-----").is_err());
    assert!(validate_ca("").is_ok());
}

#[test]
fn workspace_dashboards_validates_coordinates_and_retains_zero() {
    use crate::connection::model::EndpointCoordinates;

    let mut draft = draft_for("https://storage.example.test", "research", "key");
    let origin = EndpointCoordinates {
        latitude: 0.0,
        longitude: 0.0,
    };
    draft.settings.as_mut().unwrap().coordinates = Some(origin);
    let profile = validate_profile(&draft, None, &[]).unwrap();
    assert!(profile.settings.coordinates == Some(origin));

    draft.settings.as_mut().unwrap().coordinates = Some(EndpointCoordinates {
        latitude: 90.01,
        longitude: 0.0,
    });
    assert!(validate_profile(&draft, None, &[]).is_err());
}

#[test]
fn connection_proxy_accepts_an_origin_without_exposing_rejected_credentials() {
    assert_eq!(normalize_proxy_url(" ").unwrap(), "");
    assert_eq!(
        normalize_proxy_url(" http://127.0.0.1:33128/ ").unwrap(),
        "http://127.0.0.1:33128",
    );
    for invalid in [
        "socks5://proxy.example.test:1080",
        "http://user:fixture-secret@proxy.example.test",
        "http://proxy.example.test/path",
        "http://proxy.example.test?token=fixture-secret",
        "http://proxy.example.test#fragment",
    ] {
        let error = normalize_proxy_url(invalid).unwrap_err();
        assert!(!error.to_string().contains("fixture-secret"));
    }
}

#[tokio::test]
async fn connection_proxy_routes_connection_checks_and_signed_calls_through_https_connect() {
    use crate::common::signing::{SignedClient, request};
    use crate::transfer::security::write_client;

    let (proxy, requests) = mock_endpoint(vec![("403 Forbidden", ""), ("403 Forbidden", "")]);
    let endpoint = "https://storage-proxy.example.invalid";
    let directory = tempfile::tempdir().unwrap();
    let service = ConnectionService::with_credentials(
        ConnectionRepository::open(directory.path().into()).unwrap(),
        MockKeyring::default(),
    );
    let mut draft = draft_for(endpoint, "tenant", "test-access");
    draft.settings.as_mut().unwrap().proxy_url = proxy.clone();
    let profile = service.save(draft).await.unwrap();
    assert_eq!(service.list().unwrap()[0].settings.proxy_url, proxy);
    let session = service.session(profile.id).unwrap();
    assert!(
        write_client(&session.s3)
            .head_bucket()
            .bucket("archive")
            .send()
            .await
            .is_err()
    );
    let client = SignedClient::for_connection(&session.profile).unwrap();
    let request = request(
        reqwest::Method::GET,
        Url::parse(endpoint).unwrap(),
        None,
        Vec::new(),
    )
    .unwrap();
    assert!(client.send(&session, "s3", request).await.is_err());
    let requests = requests.join().unwrap();
    assert!(requests.iter().all(|request| {
        request.starts_with("CONNECT storage-proxy.example.invalid:443 HTTP/1.1")
            && !request.to_ascii_lowercase().contains("authorization:")
    }));
}

#[tokio::test]
async fn connection_proxy_preserves_a_successful_check_without_affecting_other_profiles() {
    let (proxy, requests) = mock_endpoint(vec![("200 OK", "")]);
    let (direct, direct_requests) = mock_endpoint(vec![("200 OK", "")]);
    let directory = tempfile::tempdir().unwrap();
    let service = ConnectionService::with_credentials(
        ConnectionRepository::open(directory.path().into()).unwrap(),
        MockKeyring::default(),
    );
    let mut draft = draft_for(
        "http://proxy-only.example.invalid",
        "proxied",
        "proxy-access",
    );
    draft.settings.as_mut().unwrap().proxy_url = proxy;
    draft.validate_s3 = true;
    assert_eq!(service.check(draft).await.unwrap().bucket_count, Some(1));
    let mut separate = draft_for(&direct, "direct", "direct-access");
    separate.validate_s3 = true;
    assert_eq!(service.check(separate).await.unwrap().bucket_count, Some(1));
    assert!(
        requests.join().unwrap()[0].starts_with("HEAD http://proxy-only.example.invalid/archive")
    );
    assert!(direct_requests.join().unwrap()[0].starts_with("HEAD /archive"));
}
