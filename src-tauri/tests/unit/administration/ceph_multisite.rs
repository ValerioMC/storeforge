use super::*;
use crate::administration::rgw_identities::tests::{Keyring, gateway};
use crate::connection::{
    model::StoredConnection,
    repository::{ConnectionRepository, CredentialRepository},
};
use crate::{
    connection::{
        model::Secret,
        service::{describe_context, open_session},
    },
    object::service::tests::object_drawer_mock,
};
use uuid::Uuid;

const LOCAL: &str = r#"{"id":"zone-a","name":"Primary","realm_id":"realm-a","system_key":{"secret_key":"fixture-zone-secret"},"endpoints":["https://unregistered.test"]}"#;
const PERIOD: &str = r#"{
    "id":"period-a","realm_id":"realm-a","epoch":4,"master_zone":"zone-a",
    "period_map":{"zonegroups":[{"id":"group-a","name":"Group","zones":[
        {"id":"zone-a","name":"Primary","sync_from_all":true},
        {"id":"zone-b","name":"Replica","sync_from_all":false,"sync_from":["Primary"]},
        {"id":"zone-c","name":"Archive","sync_from_all":false}
    ]}]}}"#;

fn response(status: &'static str, body: &str) -> (&'static str, String, Vec<u8>) {
    (
        status,
        format!("Content-Length: {}\r\n", body.len()),
        body.as_bytes().to_vec(),
    )
}

fn session(endpoint: &str) -> Session {
    let profile: Connection = serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(), "revision": Uuid::new_v4(), "name": "Fixture", "provider": "ceph",
        "endpoint": endpoint, "region": "us-east-1", "pathStyle": true, "knownBuckets": [],
        "services": { "cephAdmin": { "principalId": Uuid::new_v4(), "endpoint": endpoint,
            "enabled": true, "managedExternally": false, "capability": "detected" } }
    }))
    .unwrap();
    let context = describe_context(&profile, Workspace::CephAdmin, IDENTITY).unwrap();
    open_session(
        profile,
        context,
        Secret {
            access_key: "fixture-key".into(),
            secret_key: "fixture-secret".into(),
            session_token: None,
        },
    )
    .unwrap()
}

fn zone(session: &Session) -> ZoneBinding {
    ZoneBinding {
        scope: profile_scope(&session.profile),
        zone: "zone-a".into(),
        realm: "realm-a".into(),
        period: "period-a".into(),
        epoch: Some("4".into()),
    }
}

#[tokio::test]
async fn sends_only_safe_local_zone_fields_over_ipc() {
    let (endpoint, requests) = object_drawer_mock(vec![response("200 OK", LOCAL)]);
    let session = session(&endpoint);
    let observed = rgw::local_zone(&RgwClient::new(&session).unwrap()).await;
    let json = serde_json::to_value(observed).unwrap();
    assert_eq!(
        json["data"],
        serde_json::json!({"id":"zone-a","name":"Primary","realmId":"realm-a"})
    );
    assert!(!json.to_string().contains("fixture-zone-secret"));
    assert!(requests.join().unwrap()[0].starts_with("GET /admin/config?format=json&type=zone "));
}

#[test]
fn uses_period_zone_ids_and_reports_shard_coverage_without_estimation() {
    let period = project::period(serde_json::from_str(PERIOD).unwrap()).unwrap();
    assert!(permitted_data_flow(&period, "zone-a", "zone-b"));
    assert!(!permitted_data_flow(&period, "zone-a", "zone-c"));
    let sync = project::flow(
        serde_json::json!({
            "info": {"status": "sync", "num_shards": 8, "period": "period-a"},
            "markers": [{"key":"1","val":{"state":1,"marker":"opaque-marker"}}]
        }),
        FlowFamily::Metadata,
    )
    .unwrap();
    assert_eq!(sync.phase, "sync");
    assert_eq!(sync.expected_shards, Some(8));
    assert_eq!(sync.shards.len(), 1);
    assert_eq!(sync.shards[0].id, 1);
}

#[tokio::test]
async fn keeps_a_data_journal_unavailable_and_ignores_the_provider_error_body() {
    let (endpoint, requests) = object_drawer_mock(vec![
        response("200 OK", LOCAL),
        response("200 OK", PERIOD),
        response(
            "503 Service Unavailable",
            r#"{"message":"fixture-zone-secret"}"#,
        ),
    ]);
    let session = session(&endpoint);
    let scope = JournalScope {
        target: zone(&session),
        journal: Journal::Data { shard: 4 },
        marker: None,
        limit: 25,
    };
    let result = read_journal(&RgwClient::new(&session).unwrap(), &scope)
        .await
        .unwrap();
    assert_eq!(result.availability, Availability::Unavailable);
    assert_eq!(result.status, Some(503));
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("fixture-zone-secret")
    );
    assert!(requests.join().unwrap()[2].contains("type=data&id=4&extra-info=true&max-entries=25"));
}

#[tokio::test]
async fn carries_marker_and_exact_uint64_generation_for_a_tenant_bucket_journal() {
    let (endpoint, requests) = object_drawer_mock(vec![
        response("200 OK", LOCAL),
        response("200 OK", PERIOD),
        response(
            "200 OK",
            r#"{"bucket":"archive","tenant":"research","id":"instance-a"}"#,
        ),
        response(
            "200 OK",
            r#"{"truncated":true,"entries":[{"op_id":"marker-after","key":"notes.txt","data":{"secret_key":"fixture-zone-secret"}}],"next_log":{"generation":18446744073709551615,"num_shards":3}}"#,
        ),
    ]);
    let session = session(&endpoint);
    let scope = JournalScope {
        target: zone(&session),
        journal: Journal::Bilog {
            bucket: BucketSelector {
                name: "archive".into(),
                tenant: "research".into(),
                instance: Some("instance-a".into()),
            },
            generation: "18446744073709551614".into(),
            shard: Some(2),
        },
        marker: Some("marker /+".into()),
        limit: 10,
    };
    let journal = read_journal(&RgwClient::new(&session).unwrap(), &scope)
        .await
        .unwrap();
    let journal = journal.data.unwrap();
    assert_eq!(journal.next_marker.as_deref(), Some("marker-after"));
    assert_eq!(
        journal.next_generation.as_ref().unwrap().generation,
        "18446744073709551615"
    );
    assert_eq!(journal.bucket.as_ref().unwrap().instance, "instance-a");
    assert!(
        !serde_json::to_string(&journal)
            .unwrap()
            .contains("fixture-zone-secret")
    );
    let requests = requests.join().unwrap();
    assert!(requests.iter().all(|request| request.starts_with("GET ")));
    assert!(requests[2].contains("bucket=research%2Farchive"));
    assert!(requests[3].contains(
        "bucket-instance=archive%3Ainstance-a%3A2&tenant=research&generation=18446744073709551614"
    ));
    assert!(requests[3].contains("marker=marker+%2F%2B"));
}

#[tokio::test]
async fn stops_at_stale_bucket_or_period_bindings() {
    let (endpoint, requests) = object_drawer_mock(vec![
        response("200 OK", LOCAL),
        response("200 OK", PERIOD),
        response(
            "200 OK",
            r#"{"bucket":"archive","tenant":"research","id":"new-instance"}"#,
        ),
    ]);
    let session = session(&endpoint);
    let scope = JournalScope {
        target: zone(&session),
        journal: Journal::Bilog {
            bucket: BucketSelector {
                name: "archive".into(),
                tenant: "research".into(),
                instance: Some("old-instance".into()),
            },
            generation: "3".into(),
            shard: None,
        },
        marker: None,
        limit: 10,
    };
    let result = read_journal(&RgwClient::new(&session).unwrap(), &scope)
        .await
        .unwrap();
    assert_eq!(result.availability, Availability::Stale);
    assert!(
        !requests
            .join()
            .unwrap()
            .iter()
            .any(|request| request.starts_with("GET /admin/log"))
    );
}

#[test]
fn refuses_repeated_markers_and_implicit_generation_changes() {
    assert!(
        project::journal(
            serde_json::json!({"entries":[],"truncated":true,"marker":"same"}),
            false,
            Some("same"),
            100
        )
        .is_none()
    );
    let session = session("http://127.0.0.1:1");
    let mut scope = JournalScope {
        target: zone(&session),
        journal: Journal::Bilog {
            bucket: BucketSelector {
                name: "archive".into(),
                tenant: String::new(),
                instance: None,
            },
            generation: "18446744073709551616".into(),
            shard: None,
        },
        marker: None,
        limit: 100,
    };
    assert!(validate_journal(&scope).is_err());
    if let Journal::Bilog { generation, .. } = &mut scope.journal {
        *generation = "5".into();
    }
    assert!(validate_journal(&scope).is_ok());
    scope.marker = Some("marker".into());
    assert!(validate_journal(&scope).is_err());
}

#[tokio::test]
async fn discovers_registered_zones_and_reads_exact_bucket_instances_across_tenants() {
    let (primary, primary_requests) = gateway(7, |method, path, parameters| {
        assert_eq!(method, "GET");
        match path {
            "/admin/config" => (200, LOCAL.into()),
            "/admin/realm" => (
                200,
                r#"{"id":"realm-a","name":"Realm","current_period":"period-a"}"#.into(),
            ),
            "/admin/realm/period" => (200, PERIOD.into()),
            "/admin/bucket" => {
                assert_eq!(parameters["bucket"], "research/archive");
                (
                    200,
                    r#"{"bucket":"archive","tenant":"research","id":"src-instance"}"#.into(),
                )
            }
            "/admin/log" => {
                assert_eq!(parameters["bucket-instance"], "archive:src-instance");
                assert_eq!(parameters["tenant"], "research");
                (
                    200,
                    r#"{"oldest_gen":1,"latest_gen":2,"generations":[{"gen":2,"num_shards":3}]}"#
                        .into(),
                )
            }
            _ => panic!("Unexpected RGW request"),
        }
    });
    let (replica, replica_requests) = gateway(5, |method, path, parameters| {
        assert_eq!(method, "GET");
        match path {
            "/admin/config" => (
                200,
                r#"{"id":"zone-b","name":"Replica","realm_id":"realm-a"}"#.into(),
            ),
            "/admin/realm/period" => (200, PERIOD.into()),
            "/admin/bucket" => {
                assert_eq!(parameters["bucket"], "archive/archive");
                (
                    200,
                    r#"{"bucket":"archive","tenant":"archive","id":"dst-instance"}"#.into(),
                )
            }
            "/admin/log" => {
                assert_eq!(parameters["bucket"], "archive/archive:dst-instance");
                assert_eq!(parameters["source-bucket"], "research/archive:src-instance");
                assert_eq!(parameters["source-zone"], "zone-a");
                (200, r#"{"sync_status":{"state":"incremental-sync","incremental_gen":2},"inc_status":[{"status":"incremental-sync","inc_marker":{"position":"first"}}]}"#.into())
            }
            _ => panic!("Unexpected RGW request"),
        }
    });
    let (also_replica, also_requests) = gateway(1, |method, path, _| {
        assert_eq!((method, path), ("GET", "/admin/config"));
        (
            200,
            r#"{"id":"zone-b","name":"Replica","realm_id":"realm-a"}"#.into(),
        )
    });
    let path = tempfile::tempdir().unwrap();
    let (connections, profiles) =
        fixture_connections(path.path(), [primary, replica, also_replica]);
    let discovered = topology(&connections, profiles[0]).await.unwrap();
    assert_eq!(discovered.period.data.as_ref().unwrap().zones.len(), 3);
    assert_eq!(
        discovered
            .registrations
            .iter()
            .filter(|registered| registered
                .local
                .data
                .as_ref()
                .is_some_and(|zone| zone.id == "zone-b"))
            .count(),
        2
    );
    let serialized = serde_json::to_string(&discovered).unwrap();
    assert!(!serialized.contains("fixture-zone-secret"));
    assert!(!serialized.contains("unregistered.test"));
    let result = replication(
        &connections,
        ReplicationScope {
            source: ZoneBinding {
                scope: profiles[0],
                zone: "zone-a".into(),
                realm: "realm-a".into(),
                period: "period-a".into(),
                epoch: Some("4".into()),
            },
            destination: ZoneBinding {
                scope: profiles[1],
                zone: "zone-b".into(),
                realm: "realm-a".into(),
                period: "period-a".into(),
                epoch: Some("4".into()),
            },
            source_bucket: BucketSelector {
                name: "archive".into(),
                tenant: "research".into(),
                instance: None,
            },
            destination_bucket: BucketSelector {
                name: "archive".into(),
                tenant: "archive".into(),
                instance: None,
            },
        },
    )
    .await
    .unwrap();
    assert_eq!(result.source.data.unwrap().instance, "src-instance");
    assert_eq!(result.destination.data.unwrap().instance, "dst-instance");
    assert_eq!(
        result.sync.data.unwrap().shards[0].marker.as_deref(),
        Some("first")
    );
    assert_eq!(result.bilog.data.unwrap().latest.as_deref(), Some("2"));
    primary_requests.join().unwrap();
    replica_requests.join().unwrap();
    also_requests.join().unwrap();
}

fn fixture_connections(
    directory: &std::path::Path,
    endpoints: [String; 3],
) -> (ConnectionService, Vec<ProfileScope>) {
    let store = ConnectionRepository::open(directory.to_owned()).unwrap();
    let keyring = Keyring::default();
    let mut scopes = Vec::new();
    store
        .update(|records| {
            for (index, endpoint) in endpoints.iter().enumerate() {
                let mut fixture = session(endpoint).profile;
                fixture.name = format!("Gateway {index}");
                scopes.push(profile_scope(&fixture));
                let secret_id = Uuid::new_v4();
                keyring.write(
                    secret_id,
                    &Secret {
                        access_key: "fixture-key".into(),
                        secret_key: "fixture-secret".into(),
                        session_token: None,
                    },
                )?;
                records.push(StoredConnection {
                    profile: fixture,
                    secret_id,
                    service_secret_ids: [(IDENTITY, secret_id)].into(),
                });
            }
            Ok(())
        })
        .unwrap();
    (ConnectionService::with_credentials(store, keyring), scopes)
}
