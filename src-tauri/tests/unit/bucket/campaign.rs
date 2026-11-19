use super::*;
use crate::{
    connection::{
        model::{Connection, IdentityKind, Secret, Workspace},
        service::{describe_context, open_session},
    },
    object::service::tests::{object_drawer_client, object_drawer_mock},
};

fn session(endpoint: &str) -> Session {
    let profile: Connection = serde_json::from_value(serde_json::json!({
        "id": uuid::Uuid::new_v4(), "name": "Fixture", "provider": "compatible",
        "endpoint": endpoint, "region": "us-east-1", "pathStyle": true, "knownBuckets": []
    }))
    .unwrap();
    let context = describe_context(&profile, Workspace::StorageOps, IdentityKind::S3).unwrap();
    let mut session = open_session(
        profile,
        context,
        Secret {
            access_key: "fixture".into(),
            secret_key: "test-key".into(),
            session_token: None,
        },
    )
    .unwrap();
    session.s3 = object_drawer_client(endpoint);
    session
}

fn response(status: &'static str, body: &str) -> (&'static str, String, Vec<u8>) {
    (
        status,
        format!("Content-Length: {}\r\n", body.len()),
        body.as_bytes().to_vec(),
    )
}

#[tokio::test]
async fn refuses_to_write_when_provider_changed_since_review() {
    let (endpoint, requests) = object_drawer_mock(vec![response(
        "200 OK",
        "<LifecycleConfiguration><Rule/></LifecycleConfiguration>",
    )]);
    let result = apply_configuration(
        &session(&endpoint),
        "records",
        ConfigurationUpdate {
            configuration: Configuration::Lifecycle,
            expected: String::new(),
            change: ConfigurationChange::Remove,
        },
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("changed"));
    let requests = requests.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /records?lifecycle "));
}

#[tokio::test]
async fn applies_only_after_read_and_returns_the_provider_readback() {
    let desired = "<NotificationConfiguration><TopicConfiguration><Id>example</Id></TopicConfiguration></NotificationConfiguration>";
    let before = "<NotificationConfiguration/>";
    let (endpoint, requests) = object_drawer_mock(vec![
        response("200 OK", before),
        response("200 OK", ""),
        response("200 OK", desired),
    ]);
    let result = apply_configuration(
        &session(&endpoint),
        "records",
        ConfigurationUpdate {
            configuration: Configuration::Notification,
            expected: before.into(),
            change: ConfigurationChange::Write(desired.into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(result, desired);
    let requests = requests.join().unwrap();
    assert!(requests[0].starts_with("GET /records?notification "));
    assert!(requests[1].starts_with("PUT /records?notification "));
    assert!(requests[2].starts_with("GET /records?notification "));
}

#[tokio::test]
async fn recalculates_only_the_scoped_bucket_usage() {
    let reply = r#"<ListVersionsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><IsTruncated>false</IsTruncated><Version><Key>object.txt</Key><VersionId>v1</VersionId><IsLatest>true</IsLatest><Size>157</Size></Version></ListVersionsResult>"#;
    let (endpoint, requests) = object_drawer_mock(vec![response("200 OK", reply)]);
    let reading = recalculate_usage(&session(&endpoint), "records".into()).await;
    assert_eq!(reading.value.unwrap().current_bytes, Some(157));
    let requests = requests.join().unwrap();
    let request = requests[0].lines().next().unwrap();
    let mut parts = request.split_whitespace();
    assert_eq!(parts.next(), Some("GET"));
    let uri: http::Uri = parts.next().unwrap().parse().unwrap();
    assert_eq!(uri.path().trim_end_matches('/'), "/records");
    assert!(
        uri.query()
            .unwrap()
            .split('&')
            .any(|part| part.starts_with("versions"))
    );
}
