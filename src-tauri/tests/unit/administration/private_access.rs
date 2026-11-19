use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use super::*;
use crate::{
    audit::service::AuditService,
    connection::{
        model::{ConnectionDraft, Provider},
        repository::{ConnectionRepository, CredentialRepository},
    },
};

#[derive(Clone, Default)]
struct TestKeyring(Arc<Mutex<BTreeMap<Uuid, Secret>>>);

impl CredentialRepository for TestKeyring {
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

fn endpoint(
    expected: &[&'static str],
    provider: impl Fn(&BTreeMap<String, String>, usize) -> (u16, String) + Send + 'static,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let expected = expected.to_vec();
    (
        url,
        thread::spawn(move || {
            for (index, expected_action) in expected.into_iter().enumerate() {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("Missing {expected_action}: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let fields = {
                    let mut reader = BufReader::new(&mut stream);
                    let mut content_length = 0;
                    loop {
                        let mut line = String::new();
                        reader.read_line(&mut line).unwrap();
                        if line.trim().is_empty() {
                            break;
                        }
                        if let Some((key, value)) = line.split_once(':')
                            && key.eq_ignore_ascii_case("content-length")
                        {
                            content_length = value.trim().parse().unwrap();
                        }
                    }
                    let mut body = vec![0; content_length];
                    reader.read_exact(&mut body).unwrap();
                    url::form_urlencoded::parse(&body)
                        .into_owned()
                        .collect::<BTreeMap<_, _>>()
                };
                assert_eq!(fields["Action"], expected_action);
                let (status, body) = provider(&fields, index);
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        }),
    )
}

fn response(action: &str, body: &str) -> String {
    format!(
        "<{action}Response xmlns=\"urn:iam\"><{action}Result>{body}</{action}Result></{action}Response>"
    )
}

fn user_response(action: &str, fields: &BTreeMap<String, String>) -> String {
    let name = &fields["UserName"];
    let id = name.strip_prefix(RESERVED_PREFIX).unwrap();
    response(
        action,
        &format!(
            "<User><UserId>remote-id</UserId><UserName>{name}</UserName><Path>/storeforge-private/{id}/</Path></User>"
        ),
    )
}

async fn administrator(connections: &ConnectionService, endpoint: String) -> Uuid {
    connections
        .save(ConnectionDraft {
            id: None,
            name: "IAM Administrator".into(),
            provider: Provider::Compatible,
            endpoint,
            region: "us-east-1".into(),
            path_style: true,
            known_buckets: Vec::new(),
            credentials: Some(Secret {
                access_key: "parent-key".into(),
                secret_key: "parent-test-secret".into(),
                session_token: None,
            }),
            settings: None,
            validate_s3: false,
            reconcile: BTreeMap::new(),
        })
        .await
        .unwrap()
        .id
}

fn draft(groups: Vec<String>) -> PrivateAccessChange {
    PrivateAccessChange::Create {
        draft: PrivateAccessDraft {
            name: "Private work".into(),
            browser: true,
            manager: false,
            groups,
            policies: Vec::new(),
            inline_policies: Vec::new(),
        },
    }
}

#[tokio::test]
async fn iam_identities_managed_user_is_protected_at_the_service_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let connections = ConnectionService::with_credentials(
        ConnectionRepository::open(directory.path().into()).unwrap(),
        TestKeyring::default(),
    );
    let administrator = administrator(&connections, "http://127.0.0.1:1".into()).await;
    let session = connections
        .session_for(administrator, Workspace::Manager, IdentityKind::S3)
        .unwrap();
    let audit = AuditService::open(directory.path().into()).unwrap();
    let result = AdministrationService::new()
        .unwrap()
        .execute(
            &session,
            super::super::model::AdminRequest {
                operation: "iam-users-rename-user".into(),
                values: BTreeMap::from([
                    ("UserName".into(), "STOREFORGE-PRIVATE-remote".into()),
                    ("NewUserName".into(), "other".into()),
                ]),
                confirmed: true,
            },
            &audit,
        )
        .await;
    assert!(
        matches!(result, Err(AppError::Invalid(message)) if message.contains("personal access"))
    );
}

#[tokio::test]
async fn iam_identities_compensates_failed_membership_and_keeps_a_retry_record() {
    let (url, server) = endpoint(
        &[
            "CreateUser",
            "AddUserToGroup",
            "GetUser",
            "ListAccessKeys",
            "ListGroupsForUser",
            "ListAttachedUserPolicies",
            "ListUserPolicies",
            "DeleteUser",
        ],
        |fields, _| {
            let action = fields["Action"].as_str();
            match action {
                "CreateUser" | "GetUser" => (200, user_response(action, fields)),
                "AddUserToGroup" => (403, "Membership denied".into()),
                "ListAccessKeys" => (
                    200,
                    response(
                        action,
                        "<IsTruncated>false</IsTruncated><AccessKeyMetadata/>",
                    ),
                ),
                "ListGroupsForUser" => (
                    200,
                    response(action, "<IsTruncated>false</IsTruncated><Groups/>"),
                ),
                "ListAttachedUserPolicies" => (
                    200,
                    response(
                        action,
                        "<IsTruncated>false</IsTruncated><AttachedPolicies/>",
                    ),
                ),
                "ListUserPolicies" => (
                    200,
                    response(action, "<IsTruncated>false</IsTruncated><PolicyNames/>"),
                ),
                _ => (200, response(action, "")),
            }
        },
    );
    let directory = tempfile::tempdir().unwrap();
    let connections = ConnectionService::with_credentials(
        ConnectionRepository::open(directory.path().into()).unwrap(),
        TestKeyring::default(),
    );
    let administrator = administrator(&connections, url).await;
    let service = PrivateAccessService::new(directory.path().into());
    let result = service
        .change(
            &AdministrationService::new().unwrap(),
            &connections,
            administrator,
            draft(vec!["denied".into()]),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(result.status == PrivateAccessStatus::SetupIncomplete);
    assert!(result.issue.unwrap().contains("Membership denied"));
    assert_eq!(service.list(administrator).await.unwrap()[0].id, result.id);
    assert_eq!(connections.list().unwrap().len(), 1);
    server.join().unwrap();
}

#[tokio::test]
async fn iam_identities_reopens_with_keyring_and_retries_retirement() {
    let (url, server) = endpoint(
        &[
            "CreateUser",
            "CreateAccessKey",
            "GetUser",
            "ListAccessKeys",
            "DeleteAccessKey",
            "GetUser",
            "ListAccessKeys",
            "DeleteAccessKey",
            "ListGroupsForUser",
            "ListAttachedUserPolicies",
            "ListUserPolicies",
            "DeleteUser",
        ],
        |fields, index| {
            let action = fields["Action"].as_str();
            match action {
                "CreateUser" | "GetUser" => (200, user_response(action, fields)),
                "CreateAccessKey" => (
                    200,
                    response(
                        action,
                        &format!(
                            "<AccessKey><UserName>{}</UserName><AccessKeyId>private-key</AccessKeyId><SecretAccessKey>test-one-time-secret</SecretAccessKey></AccessKey>",
                            fields["UserName"],
                        ),
                    ),
                ),
                "ListAccessKeys" => (
                    200,
                    response(
                        action,
                        "<IsTruncated>false</IsTruncated><AccessKeyMetadata><member><AccessKeyId>private-key</AccessKeyId></member></AccessKeyMetadata>",
                    ),
                ),
                "DeleteAccessKey" if index == 4 => (403, "Revocation denied".into()),
                "ListGroupsForUser" => (
                    200,
                    response(action, "<IsTruncated>false</IsTruncated><Groups/>"),
                ),
                "ListAttachedUserPolicies" => (
                    200,
                    response(
                        action,
                        "<IsTruncated>false</IsTruncated><AttachedPolicies/>",
                    ),
                ),
                "ListUserPolicies" => (
                    200,
                    response(action, "<IsTruncated>false</IsTruncated><PolicyNames/>"),
                ),
                _ => (200, response(action, "")),
            }
        },
    );
    let directory = tempfile::tempdir().unwrap();
    let connections = ConnectionService::with_credentials(
        ConnectionRepository::open(directory.path().into()).unwrap(),
        TestKeyring::default(),
    );
    let administrator = administrator(&connections, url).await;
    let service = PrivateAccessService::new(directory.path().into());
    let iam = AdministrationService::new().unwrap();
    let created = service
        .change(&iam, &connections, administrator, draft(Vec::new()))
        .await
        .unwrap()
        .unwrap();
    assert!(created.status == PrivateAccessStatus::Active);
    let reopened = connections.session(created.id).unwrap();
    assert_eq!(
        reopened.credentials.secret_access_key(),
        "test-one-time-secret"
    );
    assert_eq!(
        reopened.profile.settings.managed_iam.unwrap().user_id,
        "remote-id"
    );
    assert!(connections.remove(created.id).is_err());
    for name in ["connections.json", "iam-private-access.json"] {
        let document = std::fs::read_to_string(directory.path().join(name)).unwrap();
        assert!(!document.contains("test-one-time-secret"));
    }
    let service = PrivateAccessService::new(directory.path().into());
    let result = service
        .change(
            &iam,
            &connections,
            administrator,
            PrivateAccessChange::Retire { id: created.id },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(result.status == PrivateAccessStatus::Retiring);
    assert!(result.issue.unwrap().contains("Revocation denied"));
    assert!(
        service
            .change(
                &iam,
                &connections,
                administrator,
                PrivateAccessChange::Retry { id: created.id }
            )
            .await
            .unwrap()
            .is_none()
    );
    assert!(connections.session(created.id).is_err());
    assert!(service.list(administrator).await.unwrap().is_empty());
    server.join().unwrap();
}
