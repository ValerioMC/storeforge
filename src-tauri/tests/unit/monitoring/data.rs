use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, TimeDelta, Utc};
use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::{
        model::{
            CapabilityState, Connection, EndpointSettings, IdentityKind, Provider, Secret,
            StoredConnection, Workspace,
        },
        repository::{ConnectionRepository, CredentialRepository},
        service::{ConnectionService, describe_context, open_session},
    },
    object::model::Location,
};

use super::{
    collection::bucket_usage,
    health::probe,
    model::{CheckMode, Coverage, HealthStatus, Origin, Reading, Scope, Source, UsageSample},
    repository::MonitoringRepository,
    service::MonitoringService,
};

const VERSION_PAGE: &str = r#"<ListVersionsResult><IsTruncated>true</IsTruncated><NextKeyMarker>test.pdf</NextKeyMarker><NextVersionIdMarker>old</NextVersionIdMarker>
<Version><Key>test.pdf</Key><VersionId>current</VersionId><IsLatest>true</IsLatest><LastModified>2026-01-01T00:00:00Z</LastModified><Size>20</Size><StorageClass>STANDARD</StorageClass></Version>
<Version><Key>test.pdf</Key><VersionId>old</VersionId><IsLatest>false</IsLatest><LastModified>2025-12-19T00:00:00Z</LastModified><Size>10</Size><StorageClass>GLACIER</StorageClass></Version>
<DeleteMarker><Key>gone</Key><VersionId>deleted</VersionId><IsLatest>true</IsLatest></DeleteMarker></ListVersionsResult>"#;
const DENIED: &str = "<Error><Code>AccessDenied</Code><Message>Denied</Message></Error>";
const EMPTY: &str = "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>";

fn clock() -> DateTime<Utc> {
    "2026-01-01T12:00:00Z".parse().unwrap()
}

fn profile(endpoint: String) -> Connection {
    Connection {
        id: Uuid::new_v4(),
        endpoint_id: Some(Uuid::new_v4()),
        revision: Some(Uuid::new_v4()),
        name: "Fixture".into(),
        endpoint,
        provider: Provider::Ceph,
        region: "us-east-1".into(),
        path_style: true,
        known_buckets: vec!["archive".into()],
        settings: EndpointSettings {
            tenant: "research".into(),
            ..Default::default()
        },
        s3_capability: CapabilityState::Detected,
        services: Default::default(),
    }
}

struct FixtureCredentials;

impl CredentialRepository for FixtureCredentials {
    fn read(&self, _id: Uuid) -> Result<Secret> {
        Ok(Secret {
            access_key: "fixture-key".into(),
            secret_key: "fixture-secret".into(),
            session_token: None,
        })
    }

    fn write(&self, _id: Uuid, _secret: &Secret) -> Result<()> {
        Err(AppError::Invalid("Unexpected credential write.".into()))
    }

    fn remove(&self, _id: Uuid) {}
}

fn mock_endpoint(
    responses: &[(&'static str, &'static str)],
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let responses = responses.to_vec();
    let worker = thread::spawn(move || {
        responses
            .into_iter()
            .map(|(status, body)| {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(3));
                        }
                        Err(error) => panic!("Mock request not received: {error}"),
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
    (endpoint, worker)
}

#[tokio::test]
async fn preserves_partial_version_measurements_and_distributions() {
    let (endpoint, worker) = mock_endpoint(&[("200 OK", VERSION_PAGE), ("403 Forbidden", DENIED)]);
    let profile = profile(endpoint);
    let id = profile.id;
    let context = describe_context(&profile, Workspace::Manager, IdentityKind::S3).unwrap();
    let session = open_session(profile, context, FixtureCredentials.read(id).unwrap()).unwrap();
    let location = Location {
        connection: id,
        bucket: "archive".into(),
        prefix: String::new(),
    };
    let usage = bucket_usage(&session.s3, &location, clock()).await;
    let requests = worker.join().unwrap();
    assert_eq!(usage.coverage, Coverage::Partial);
    let usage = usage.value.unwrap();
    assert_eq!(usage.current_bytes, Some(20));
    assert_eq!(usage.noncurrent_bytes, Some(10));
    assert_eq!(usage.delete_markers, Some(1));
    assert_eq!(usage.types["application/pdf"].objects, 2);
    assert_eq!(usage.ages["7–29 days"].bytes, Some(10));
    assert_eq!(usage.storage_classes["GLACIER"].bytes, Some(10));
    assert!(requests[1].contains("version-id-marker=old"));
}

#[tokio::test]
async fn measures_empty_live_listing_without_inventing_versions_or_supervision() {
    let (endpoint, worker) = mock_endpoint(&[("403 Forbidden", DENIED), ("200 OK", EMPTY)]);
    let profile = profile(endpoint);
    let id = profile.id;
    let directory = tempfile::tempdir().unwrap();
    let repository = ConnectionRepository::open(directory.path().into()).unwrap();
    repository
        .update(|profiles| {
            profiles.push(StoredConnection {
                profile,
                secret_id: id,
                service_secret_ids: Default::default(),
            });
            Ok(())
        })
        .unwrap();
    let connections = ConnectionService::with_credentials(repository, FixtureCredentials);
    let service = MonitoringService::open(directory.path().into()).unwrap();
    let view = service.collect(&connections, id).await.unwrap();
    let samples = view.usage.unwrap();
    assert_eq!(samples.traffic.coverage, Coverage::Unavailable);
    assert_eq!(samples.traffic_origin.principal_id, None);
    assert_eq!(samples.buckets[0].usage.coverage, Coverage::CurrentOnly);
    let usage = samples.buckets[0].usage.value.as_ref().unwrap();
    assert_eq!(usage.current_bytes, Some(0));
    assert_eq!(usage.noncurrent_bytes, None);
    assert_eq!(worker.join().unwrap().len(), 2);
}

#[test]
fn reopens_a_stale_sample_with_an_explicit_gap_and_a_measured_zero() {
    let directory = tempfile::tempdir().unwrap();
    let profile = profile(String::new());
    let scope = Scope::from(&profile);
    let now = clock();
    let sample = UsageSample {
        collected_at: now,
        bucket_coverage: Reading::complete(0),
        listing_origin: Origin::new(&profile, Source::S3Listing, Some(IdentityKind::S3)),
        quota_origin: Origin::new(&profile, Source::RgwBucket, Some(IdentityKind::CephAdmin)),
        traffic_origin: Origin::new(&profile, Source::RgwUsage, Some(IdentityKind::Supervision)),
        buckets: Vec::new(),
        traffic: Reading::missing(Coverage::Unavailable, "Unavailable"),
        scoped_traffic: None,
    };
    let repository = MonitoringRepository::open(directory.path().into()).unwrap();
    repository.usage(scope.clone(), sample).unwrap();
    let repository = MonitoringRepository::open(directory.path().into()).unwrap();
    let retrieved_at = now + TimeDelta::hours(2);
    let view = repository.view(scope.clone(), retrieved_at).unwrap();
    assert!(view.stale);
    assert_eq!(view.retrieved_at, retrieved_at);
    assert!(view.hourly[0].sample.is_none());
    assert!(view.hourly[1].sample.is_none());
    assert_eq!(
        view.hourly[2].sample.as_ref().unwrap().current_bytes,
        Some(0)
    );
    assert_eq!(view.daily[0].sample.as_ref().unwrap().collected_at, now);
    assert!(
        repository
            .view(
                Scope {
                    revision: None,
                    ..scope
                },
                retrieved_at
            )
            .unwrap()
            .usage
            .is_none()
    );
}

#[tokio::test]
async fn records_slow_down_and_recovered_using_a_mocked_health_endpoint() {
    let (endpoint, worker) = mock_endpoint(&[
        ("200 OK", ""),
        ("503 Service Unavailable", ""),
        ("429 Too Many Requests", ""),
        ("200 OK", ""),
    ]);
    let profile = profile(endpoint);
    let directory = tempfile::tempdir().unwrap();
    let repository = MonitoringRepository::open(directory.path().into()).unwrap();
    let now = clock();
    for offset in 0..4 {
        let outcome = probe(&profile, CheckMode::Http, None).await;
        repository
            .health(
                &profile,
                CheckMode::Http,
                outcome,
                now + TimeDelta::minutes(offset),
            )
            .unwrap();
    }
    let view = repository
        .view((&profile).into(), now + TimeDelta::minutes(4))
        .unwrap();
    assert_eq!(
        view.health
            .iter()
            .rev()
            .map(|sample| sample.status)
            .collect::<Vec<_>>(),
        [
            HealthStatus::Up,
            HealthStatus::Down,
            HealthStatus::Slow,
            HealthStatus::Up,
        ]
    );
    assert!(view.health[1].baseline_ms.is_some());
    assert_eq!(view.incidents.len(), 1);
    assert_eq!(view.incidents[0].severity, HealthStatus::Down);
    assert_eq!(view.incidents[0].opened_at, now + TimeDelta::minutes(1));
    assert_eq!(
        view.incidents[0].recovered_at,
        Some(now + TimeDelta::minutes(3))
    );
    assert!(
        worker
            .join()
            .unwrap()
            .iter()
            .all(|request| request.starts_with("HEAD ") && !request.contains("Authorization:"))
    );
}

#[tokio::test]
async fn signs_endpoint_traffic_and_tenant_bucket_quota_reads_with_distinct_identities() {
    use crate::connection::model::ServiceIdentity;

    const TRAFFIC: &str = r#"{"summary":[{"total":{"bytes_sent":0,"bytes_received":25,"ops":4,"successful_ops":3}}]}"#;
    const QUOTA: &str = r#"{"bucket_quota":{"enabled":true,"max_size_kb":64,"max_objects":-1},"usage":{"rgw.main":{"size":20,"num_objects":2}}}"#;
    let (traffic_endpoint, traffic_worker) = mock_endpoint(&[("200 OK", TRAFFIC)]);
    let (quota_endpoint, quota_worker) = mock_endpoint(&[("200 OK", QUOTA)]);
    let mut profile = profile(String::new());
    let mut sessions = Vec::new();
    for (identity, endpoint, key) in [
        (
            IdentityKind::Supervision,
            traffic_endpoint,
            "supervision-key",
        ),
        (IdentityKind::CephAdmin, quota_endpoint, "ceph-admin-key"),
    ] {
        profile.services.insert(
            identity,
            ServiceIdentity {
                principal_id: Uuid::new_v4(),
                endpoint: endpoint.clone(),
                enabled: true,
                managed_externally: false,
                capability: CapabilityState::Detected,
            },
        );
        let context = describe_context(&profile, Workspace::CephAdmin, identity).unwrap();
        let mut scoped_profile = profile.clone();
        scoped_profile.endpoint = endpoint;
        let mut secret = FixtureCredentials.read(profile.id).unwrap();
        secret.access_key = key.into();
        sessions.push(open_session(scoped_profile, context, secret).unwrap());
    }

    let traffic = super::rgw::traffic(&sessions[0], clock(), None).await;
    assert_eq!(traffic.coverage, Coverage::Complete);
    assert_eq!(traffic.value.unwrap().totals.bytes_sent, Some(0));
    let (quota, storage) = super::rgw::bucket_info(&sessions[1], "archive").await;
    assert_eq!(quota.value.unwrap().max_bytes, Some(64 * 1024));
    assert_eq!(storage.value.unwrap().objects, 2);

    let traffic_request = traffic_worker.join().unwrap().remove(0);
    let quota_request = quota_worker.join().unwrap().remove(0);
    assert!(traffic_request.contains("Credential=supervision-key/"));
    assert!(traffic_request.contains("show-summary=true"));
    assert!(quota_request.contains("Credential=ceph-admin-key/"));
    assert!(quota_request.contains("bucket=research%2Farchive"));
}

#[tokio::test]
async fn workspace_dashboards_scoped_traffic_excludes_unrelated_or_unidentified_owners() {
    const TRAFFIC: &str = r#"{"summary":[
        {"user":"research$alice","total":{"bytes_sent":0,"bytes_received":25,"ops":4,"successful_ops":3}},
        {"user":"other$alice","total":{"bytes_sent":500,"bytes_received":90,"ops":7,"successful_ops":5}},
        {"total":{"bytes_sent":200,"bytes_received":50,"ops":3,"successful_ops":0}}]}"#;
    let (endpoint, worker) = mock_endpoint(&[("200 OK", TRAFFIC)]);
    let profile = profile(endpoint);
    let context = describe_context(&profile, Workspace::CephAdmin, IdentityKind::S3).unwrap();
    let id = profile.id;
    let session = open_session(profile, context, FixtureCredentials.read(id).unwrap()).unwrap();
    let reading = super::rgw::traffic(&session, clock(), Some("research$alice")).await;

    let sample = reading.value.unwrap();
    assert_eq!(reading.coverage, Coverage::Complete);
    assert_eq!(sample.totals.bytes_sent, Some(0));
    assert_eq!(sample.totals.requests, Some(4));
    assert!(worker.join().unwrap()[0].contains("uid=research%24alice"));
}

#[test]
fn workspace_dashboards_retains_quota_utilization_when_storage_and_limits_are_measured() {
    use super::model::{BucketSample, Quota, RgwStorage};

    let profile = profile(String::new());
    let mut sample = UsageSample {
        collected_at: clock(),
        listing_origin: Origin::new(&profile, Source::S3Listing, Some(IdentityKind::S3)),
        quota_origin: Origin::new(&profile, Source::RgwBucket, Some(IdentityKind::CephAdmin)),
        traffic_origin: Origin::new(&profile, Source::RgwUsage, Some(IdentityKind::Supervision)),
        bucket_coverage: Reading::complete(1),
        buckets: vec![BucketSample {
            bucket: "archive".into(),
            collected_at: clock(),
            usage: Reading::missing(Coverage::Denied, "Access denied"),
            quota: Reading::complete(Quota {
                enabled: true,
                max_bytes: Some(200),
                max_objects: None,
            }),
            rgw_storage: Reading::complete(RgwStorage {
                bytes: 50,
                objects: 2,
            }),
        }],
        traffic: Reading::missing(Coverage::Unavailable, "No Supervision identity"),
        scoped_traffic: None,
    };

    assert_eq!(super::history::point(&sample).quota_usage, Some(0.25));
    sample.buckets[0].rgw_storage = Reading::complete(RgwStorage {
        bytes: 0,
        objects: 0,
    });
    assert_eq!(super::history::point(&sample).quota_usage, Some(0.0));
    sample.bucket_coverage.coverage = Coverage::Partial;
    assert_eq!(super::history::point(&sample).quota_usage, None);
}
