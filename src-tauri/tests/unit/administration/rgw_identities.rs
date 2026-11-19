use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use chrono::{TimeDelta, Utc};
use url::Url;
use uuid::Uuid;

use super::{
    RgwIdentityService,
    model::*,
    repository::{RegistryRepository, issued_key, parse_identity},
};
use crate::{
    common::error::{AppError, Result},
    connection::{
        model::{
            CapabilityState, Connection, EndpointSettings, IdentityKind as ConnectionIdentity,
            Provider, Secret, ServiceIdentity, StoredConnection,
        },
        repository::{ConnectionRepository, CredentialRepository},
        service::ConnectionService,
    },
};

#[derive(Clone, Default)]
pub(crate) struct Keyring(Arc<Mutex<BTreeMap<Uuid, Secret>>>);

impl CredentialRepository for Keyring {
    fn read(&self, id: Uuid) -> Result<Secret> {
        self.0
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::Credential("Missing fixture key.".into()))
    }

    fn write(&self, id: Uuid, secret: &Secret) -> Result<()> {
        self.0.lock().unwrap().insert(id, secret.clone());
        Ok(())
    }

    fn remove(&self, id: Uuid) {
        self.0.lock().unwrap().remove(&id);
    }
}

type Fields = BTreeMap<String, String>;

pub(crate) fn gateway(
    requests: usize,
    mut reply: impl FnMut(&str, &str, &Fields) -> (u16, String) + Send + 'static,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    (
        endpoint,
        thread::spawn(move || {
            for _ in 0..requests {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("Expected RGW request: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let (method, target) = {
                    let mut reader = BufReader::new(&mut stream);
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let mut parts = line.split_whitespace();
                    let method = parts.next().unwrap().to_owned();
                    let target = parts.next().unwrap().to_owned();
                    loop {
                        line.clear();
                        reader.read_line(&mut line).unwrap();
                        if line.trim().is_empty() {
                            break;
                        }
                    }
                    (method, target)
                };
                let url = Url::parse(&format!("http://fixture{target}")).unwrap();
                let fields = url.query_pairs().into_owned().collect();
                let (status, body) = reply(&method, url.path(), &fields);
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        }),
    )
}

pub(crate) fn fixture(directory: &Path, endpoint: String) -> (ConnectionService, Keyring, Uuid) {
    let store = ConnectionRepository::open(directory.to_owned()).unwrap();
    let credentials = Keyring::default();
    let id = Uuid::new_v4();
    let admin = Uuid::new_v4();
    for (key, name) in [(id, "connection-key"), (admin, "admin-key")] {
        credentials
            .write(
                key,
                &Secret {
                    access_key: name.into(),
                    secret_key: "fixture-secret".into(),
                    session_token: None,
                },
            )
            .unwrap();
    }
    store
        .update(|profiles| {
            profiles.push(StoredConnection {
                profile: Connection {
                    id,
                    endpoint,
                    name: "Ceph fixture".into(),
                    provider: Provider::Ceph,
                    region: "us-east-1".into(),
                    path_style: true,
                    known_buckets: Vec::new(),
                    endpoint_id: Some(id),
                    revision: None,
                    settings: EndpointSettings::default(),
                    s3_capability: CapabilityState::Detected,
                    services: BTreeMap::from([(
                        ConnectionIdentity::AdminOps,
                        ServiceIdentity {
                            principal_id: admin,
                            endpoint: String::new(),
                            enabled: true,
                            managed_externally: false,
                            capability: CapabilityState::Detected,
                        },
                    )]),
                },
                secret_id: id,
                service_secret_ids: BTreeMap::from([(ConnectionIdentity::AdminOps, admin)]),
            });
            Ok(())
        })
        .unwrap();
    (
        ConnectionService::with_credentials(store, credentials.clone()),
        credentials,
        id,
    )
}

fn user(uid: &str, account: &str, root: bool, keys: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "user_id": uid, "display_name": uid, "account_id": account,
        "type": if root { "root" } else { "rgw" }, "suspended": 0, "keys": keys,
    })
}

#[tokio::test]
async fn rgw_identities_imports_equal_uids_and_discovers_a_custom_account_root() {
    let (endpoint, server) = gateway(7, |method, path, query| {
        assert_eq!(method, "GET");
        let document = match path {
            "/admin/user" if query.contains_key("list") => {
                if query.contains_key("marker") {
                    serde_json::json!({"keys":["blue$king"],"marker":"","truncated":false})
                } else {
                    serde_json::json!({"keys":["red$reader","blue$reader"],"marker":"blue$reader","truncated":true})
                }
            }
            "/admin/user" => {
                let uid = &query["uid"];
                assert_eq!(
                    Some(&query["tenant"][..]),
                    uid.split_once('$').map(|(tenant, _)| tenant)
                );
                if uid == "blue$king" {
                    user(uid, "RGW42", true, serde_json::json!([]))
                } else {
                    user(uid, "", false, serde_json::json!([]))
                }
            }
            "/admin/account" => serde_json::json!({"id":"RGW42","tenant":"blue","name":"Research"}),
            _ => panic!("Unexpected path {path}"),
        };
        (200, document.to_string())
    });
    let directory = tempfile::tempdir().unwrap();
    let (connections, _, connection) = fixture(directory.path(), endpoint);
    let service = RgwIdentityService::new(directory.path().into());
    let first = parse_identity("red$reader", "").unwrap();
    let second = parse_identity("blue$reader", "").unwrap();
    service
        .change(
            &connections,
            connection,
            ChangeRequest::ImportUsers {
                identities: vec![first.clone(), second.clone()],
            },
        )
        .await
        .unwrap();
    service
        .change(
            &connections,
            connection,
            ChangeRequest::ImportAccounts {
                entries: vec![AccountImport {
                    id: "RGW42".into(),
                    root: None,
                }],
            },
        )
        .await
        .unwrap();
    let read = service
        .read(
            &connections,
            connection,
            ReadRequest::List {
                kind: IdentityKind::User,
                managed: true,
                marker: None,
            },
        )
        .await
        .unwrap();
    let ReadResponse::List(list) = read else {
        panic!("Missing managed list");
    };
    assert_eq!(
        list.items
            .iter()
            .map(|row| row.target.clone())
            .collect::<Vec<_>>(),
        [
            IdentityTarget::User { identity: first },
            IdentityTarget::User { identity: second }
        ]
    );
    let read = service
        .read(
            &connections,
            connection,
            ReadRequest::List {
                kind: IdentityKind::Account,
                managed: true,
                marker: None,
            },
        )
        .await
        .unwrap();
    let ReadResponse::List(list) = read else {
        panic!("Missing managed accounts");
    };
    assert_eq!(
        list.items[0].root,
        Some(parse_identity("blue$king", "").unwrap())
    );
    server.join().unwrap();
}

#[tokio::test]
async fn rgw_identities_rejects_owned_identity_and_keeps_root_fields_out_of_quota_writes() {
    let (endpoint, server) = gateway(5, |method, path, query| {
        assert_eq!(path, "/admin/user");
        assert_eq!(query["uid"], "blue$king");
        assert_eq!(query["tenant"], "blue");
        assert!(!query.contains_key("account-root"));
        assert!(!query.contains_key("account-id"));
        if method == "PUT" {
            assert_eq!(query["quota-type"], "bucket");
            assert_eq!(query["max-size"], "1048576");
            assert!(!query.contains_key("max-objects"));
            return (200, String::new());
        }
        if method == "POST" {
            assert_eq!(query["display-name"], "Root operator");
            assert_eq!(query["email"], "");
            return (200, String::new());
        }
        (
            200,
            user("blue$king", "RGW42", true, serde_json::json!([])).to_string(),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    let (connections, _, connection) = fixture(directory.path(), endpoint);
    let service = RgwIdentityService::new(directory.path().into());
    let identity = parse_identity("blue$king", "").unwrap();
    let owned = service
        .change(
            &connections,
            connection,
            ChangeRequest::ImportUsers {
                identities: vec![identity.clone()],
            },
        )
        .await;
    assert!(
        matches!(owned, Err(AppError::Invalid(message)) if message.contains("owned by an RGW account"))
    );
    let conflict = service
        .change(
            &connections,
            connection,
            ChangeRequest::CreateUser {
                identity: identity.clone(),
                draft: UserDraft {
                    display_name: Some("Duplicate".into()),
                    email: None,
                    suspended: None,
                    max_buckets: None,
                },
                account: None,
                root: false,
                managed: false,
            },
        )
        .await;
    assert!(
        matches!(conflict, Err(AppError::Invalid(message)) if message.contains("already exists"))
    );
    service
        .change(
            &connections,
            connection,
            ChangeRequest::UpdateUser {
                identity: identity.clone(),
                draft: UserDraft {
                    display_name: Some("Root operator".into()),
                    email: Some(String::new()),
                    suspended: None,
                    max_buckets: None,
                },
            },
        )
        .await
        .unwrap();
    service
        .change(
            &connections,
            connection,
            ChangeRequest::SetQuota {
                target: IdentityTarget::User {
                    identity: identity.clone(),
                },
                bucket: true,
                quota: Quota {
                    enabled: true,
                    max_bytes: Some(1_048_576),
                    max_objects: None,
                },
            },
        )
        .await
        .unwrap();
    let read = service
        .read(&connections, connection, ReadRequest::User { identity })
        .await
        .unwrap();
    let ReadResponse::User(user) = read else {
        panic!("Missing user");
    };
    assert_eq!(user.account_root, Some(true));
    server.join().unwrap();
}

#[tokio::test]
async fn rgw_identities_issues_only_the_new_secret_and_verifies_key_state_after_refresh() {
    let mut created = false;
    let mut active = true;
    let (endpoint, server) = gateway(7, move |method, path, query| {
        assert_eq!(path, "/admin/user");
        if method == "PUT" && query["generate-key"] == "true" {
            assert_eq!(query["uid"], "reader");
            created = true;
            return (200, r#"[{"user":"reader","access_key":"connection-key","secret_key":"old-secret"},{"user":"reader","access_key":"issued-key","secret_key":"new-secret"}]"#.into());
        }
        if method == "PUT" {
            assert_eq!(query["access-key"], "issued-key");
            assert_eq!(query["generate-key"], "false");
            active = query["active"] == "true";
            return (200, String::new());
        }
        let mut keys = vec![
            serde_json::json!({"access_key":"connection-key","secret_key":"old-secret","active":true}),
        ];
        if created {
            keys.push(serde_json::json!({"access_key":"issued-key","secret_key":"new-secret","active":active}));
        }
        (
            200,
            user("reader", "", false, serde_json::json!(keys)).to_string(),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    let (connections, _, connection) = fixture(directory.path(), endpoint);
    let service = RgwIdentityService::new(directory.path().into());
    let owner = parse_identity("reader", "").unwrap();
    let initial = service
        .read(&connections, connection, ReadRequest::DelegatedKeys)
        .await
        .unwrap();
    let ReadResponse::User(initial) = initial else {
        panic!("Missing owner");
    };
    assert_eq!(initial.connection_key_id.as_deref(), Some("connection-key"));
    let request = |change| ChangeRequest::Key {
        scope: KeyScope::Delegated,
        owner: owner.clone(),
        change,
    };
    let issued = service
        .change(
            &connections,
            connection,
            request(KeyChange::Create {
                metadata: KeyMetadata {
                    name: "Reporting".into(),
                    notes: "nightly".into(),
                    expires_at: None,
                },
                private: false,
            }),
        )
        .await
        .unwrap()
        .issued
        .unwrap();
    assert_eq!(issued.access_key, "issued-key");
    assert_eq!(issued.secret_key, "new-secret");
    service
        .change(
            &connections,
            connection,
            request(KeyChange::Status {
                id: issued.access_key,
                active: false,
            }),
        )
        .await
        .unwrap();
    let reloaded = RgwIdentityService::new(directory.path().into());
    let read = reloaded
        .read(&connections, connection, ReadRequest::DelegatedKeys)
        .await
        .unwrap();
    let serialized = serde_json::to_string(&read).unwrap();
    assert!(!serialized.contains("secret"));
    let ReadResponse::User(user) = read else {
        panic!("Missing owner");
    };
    let keys = user.keys.unwrap();
    assert_eq!(keys[0].active, Some(true));
    assert_eq!(keys[1].active, Some(false));
    assert_eq!(keys[1].metadata.name, "Reporting");
    server.join().unwrap();
}

#[tokio::test]
async fn rgw_identities_reports_failed_expiration_and_retries_with_the_registered_owner() {
    let mut writes = 0;
    let (endpoint, server) = gateway(5, move |method, _, query| {
        if method == "PUT" {
            assert_eq!(query["active"], "false");
            assert_eq!(query["uid"], "blue$reader");
            writes += 1;
            return if writes == 1 {
                (403, "provider body".into())
            } else {
                (200, String::new())
            };
        }
        (
            200,
            user(
                "blue$reader",
                "",
                false,
                serde_json::json!([{"access_key":"key","active":writes < 2}]),
            )
            .to_string(),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    let (connections, _, connection) = fixture(directory.path(), endpoint.clone());
    let service = RgwIdentityService::new(directory.path().into());
    let repository = RegistryRepository::new(directory.path().into());
    repository
        .write(&Registry {
            identities: vec![],
            keys: vec![KeyRecord {
                scope: RegistryScope {
                    endpoint: connection,
                    admin_url: endpoint,
                },
                owner: parse_identity("blue$reader", "").unwrap(),
                id: "key".into(),
                administrator: connection,
                metadata: KeyMetadata {
                    expires_at: Some(Utc::now() - TimeDelta::minutes(1)),
                    ..KeyMetadata::default()
                },
                enforcement: Some(Enforcement::Pending),
                private_profile: None,
                issue: None,
            }],
        })
        .unwrap();
    service.enforce_expirations(&connections).await.unwrap();
    let failed = repository.read().unwrap();
    assert!(
        matches!(&failed.keys[0].enforcement, Some(Enforcement::Failed { message }) if message.contains("403") && !message.contains("provider body"))
    );
    service.enforce_expirations(&connections).await.unwrap();
    assert!(matches!(
        repository.read().unwrap().keys[0].enforcement,
        Some(Enforcement::Disabled)
    ));
    server.join().unwrap();
}

#[tokio::test]
async fn rgw_identities_saves_private_keys_in_the_keyring_and_clears_the_profile_on_delete() {
    let mut created = false;
    let (endpoint, server) = gateway(4, move |method, path, query| {
        assert_eq!(path, "/admin/user");
        assert_eq!(query["uid"], "blue$reader");
        if method == "PUT" {
            assert_eq!(query["generate-key"], "true");
            created = true;
            return (
                200,
                r#"{"user":"blue$reader","access_key":"private-key","secret_key":"only-keyring"}"#
                    .into(),
            );
        }
        if method == "DELETE" {
            assert_eq!(query["access-key"], "private-key");
            return (200, String::new());
        }
        let keys = if created {
            serde_json::json!([{"access_key":"private-key","active":true}])
        } else {
            serde_json::json!([])
        };
        (200, user("blue$reader", "", false, keys).to_string())
    });
    let directory = tempfile::tempdir().unwrap();
    let (connections, keyring, connection) = fixture(directory.path(), endpoint);
    let service = RgwIdentityService::new(directory.path().into());
    let owner = parse_identity("blue$reader", "").unwrap();
    let request = |change| ChangeRequest::Key {
        scope: KeyScope::Inventory,
        owner: owner.clone(),
        change,
    };
    let created = service
        .change(
            &connections,
            connection,
            request(KeyChange::Create {
                metadata: KeyMetadata {
                    name: "Scoped Browser".into(),
                    ..KeyMetadata::default()
                },
                private: true,
            }),
        )
        .await
        .unwrap();
    assert!(created.issued.is_none());
    let profile = created.profile.unwrap();
    assert_eq!(profile.settings.tenant, "blue");
    assert!(profile.services.is_empty());
    assert_eq!(profile.settings.managed_rgw.unwrap().uid, "reader");
    assert_eq!(keyring.read(profile.id).unwrap().secret_key, "only-keyring");
    let registry = std::fs::read_to_string(directory.path().join("rgw-identities.json")).unwrap();
    assert!(!registry.contains("only-keyring"));
    let ordinary = service
        .change(
            &connections,
            connection,
            request(KeyChange::Delete {
                id: "private-key".into(),
            }),
        )
        .await
        .unwrap();
    assert!(ordinary.issued.is_none());
    assert_eq!(connections.list().unwrap().len(), 1);
    assert!(keyring.read(profile.id).is_err());
    server.join().unwrap();
}

#[test]
fn rgw_identities_refuses_ambiguous_key_responses() {
    let owner = parse_identity("blue$reader", "").unwrap();
    let payload = r#"[{"user":"blue$reader","access_key":"first","secret_key":"a"},{"user":"blue$reader","access_key":"second","secret_key":"b"}]"#;
    assert!(matches!(
        issued_key(payload, &owner, &Default::default()),
        Err(AppError::AmbiguousWrite(_))
    ));
    let wrong = r#"{"user":"red$reader","access_key":"unexpected","secret_key":"a"}"#;
    assert!(issued_key(wrong, &owner, &Default::default()).is_err());
}

#[tokio::test]
async fn rgw_identities_account_usage_counts_only_buckets_owned_by_the_selected_account() {
    let (url, server) = gateway(2, |method, path, fields| {
        assert_eq!(method, "GET");
        let body = match path {
            "/admin/account" => {
                assert_eq!(fields["id"], "RGW42");
                r#"{"id":"RGW42","tenant":"research","name":"Research"}"#
            }
            "/admin/bucket" => {
                assert_eq!(fields["stats"], "true");
                r#"[
                  {"owner":"RGW42","tenant":"research","usage":{"rgw.main":{"size":320,"num_objects":2}}},
                  {"owner":"RGW81","tenant":"research","usage":{"rgw.main":{"size":8000,"num_objects":1}}},
                  {"owner":"RGW42","tenant":"finance","usage":{"rgw.main":{"size":10000,"num_objects":4}}}
                ]"#
            }
            _ => panic!("Unexpected path {path}"),
        };
        (200, body.into())
    });
    let directory = tempfile::tempdir().unwrap();
    let (connections, _, connection) = fixture(directory.path(), url);
    let service = RgwIdentityService::new(directory.path().into());
    let result = service
        .read(
            &connections,
            connection,
            ReadRequest::AccountUsage { id: "RGW42".into() },
        )
        .await
        .unwrap();
    let ReadResponse::Usage(usage) = result else {
        panic!("Expected scoped usage");
    };
    assert_eq!(
        (usage.bytes, usage.objects, usage.buckets),
        (Some(320), Some(2), Some(1))
    );
    server.join().unwrap();
}

#[tokio::test]
async fn rgw_identities_creates_and_confirms_a_managed_account_root_without_issuing_a_key() {
    let mut created_root = false;
    let (url, server) = gateway(4, move |method, path, fields| {
        let body = match (method, path) {
            ("GET", "/admin/user")
                if fields["uid"] == "research$project-admin" && !fields.contains_key("stats") =>
            {
                if !created_root {
                    return (404, r#"{"Code":"NoSuchUser"}"#.into());
                }
                user(
                    "research$project-admin",
                    "RGW42",
                    true,
                    serde_json::json!([]),
                )
                .to_string()
            }
            ("POST", "/admin/account") => {
                assert_eq!(fields["tenant"], "research");
                assert_eq!(fields["max-roles"], "12");
                r#"{"id":"RGW42","tenant":"research","name":"Research"}"#.into()
            }
            ("PUT", "/admin/user") => {
                assert_eq!(fields["uid"], "research$project-admin");
                assert_eq!(fields["account-id"], "RGW42");
                assert_eq!(fields["account-root"], "true");
                assert_eq!(fields["generate-key"], "false");
                created_root = true;
                String::new()
            }
            _ => panic!("Unexpected request {method} {path}"),
        };
        (200, body)
    });
    let directory = tempfile::tempdir().unwrap();
    let (connections, _, connection) = fixture(directory.path(), url);
    let service = RgwIdentityService::new(directory.path().into());
    let request: ChangeRequest = serde_json::from_str(r#"{
        "action":"createAccount","id":null,"tenant":"research","rootUid":"project-admin","managed":true,
        "draft":{"name":"Research","limits":{"maxRoles":12}}
    }"#).unwrap();
    let outcome = service
        .change(&connections, connection, request)
        .await
        .unwrap();
    assert!(outcome.issue.is_none());
    assert_eq!(
        outcome.target,
        Some(IdentityTarget::Account { id: "RGW42".into() })
    );
    let repository = RegistryRepository::new(directory.path().into());
    let registry = repository.read().unwrap();
    assert_eq!(
        registry.identities[0].root,
        Some(parse_identity("research$project-admin", "").unwrap())
    );
    server.join().unwrap();
}
