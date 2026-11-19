use chrono::{NaiveDate, Utc};
use uuid::Uuid;

use crate::{
    administration::rgw_identities::model::{IdentityTarget, RegistryScope, UserIdentity},
    monitoring::model::{Coverage, Reading, Traffic},
};

use super::{
    collection::traffic_window,
    model::{Document, Measurement, Rates, ScopedMeasurement},
    repository::merge_measurements,
    service::{report_range, subject_measurements},
};

fn date(value: &str) -> NaiveDate {
    value.parse().unwrap()
}

fn measurement(subject: IdentityTarget, day: &str, bytes: u64) -> ScopedMeasurement {
    ScopedMeasurement {
        scope: RegistryScope {
            endpoint: Uuid::nil(),
            admin_url: "https://storage.invalid".into(),
        },
        measurement: Measurement {
            day: date(day),
            collected_at: Utc::now(),
            endpoint_id: Uuid::nil(),
            subject,
            name: "Owned".into(),
            account_id: Some("RGW-one".into()),
            storage: Reading::complete(bytes),
            traffic: Reading::complete(Traffic {
                requests: Some(0),
                ..Default::default()
            }),
        },
    }
}

fn identity(tenant: &str) -> UserIdentity {
    UserIdentity {
        tenant: tenant.into(),
        uid: "operator".into(),
    }
}

#[test]
fn report_and_collection_use_utc_month_boundaries() {
    let range = report_range(date("2024-02-01")).unwrap();
    assert_eq!(range.start, date("2023-03-01"));
    assert_eq!(range.end, date("2024-03-01"));
    assert!(range.contains(&date("2024-02-29")));
    assert!(report_range(date("2024-02-02")).is_err());
    let now = "2024-03-01T15:30:00Z".parse().unwrap();
    let (start, end, closed) = traffic_window(date("2024-02-29"), now).unwrap();
    assert_eq!(start.to_rfc3339(), "2024-02-29T00:00:00+00:00");
    assert_eq!(end.to_rfc3339(), "2024-03-01T00:00:00+00:00");
    assert!(closed);
    let (_, end, closed) = traffic_window(date("2024-03-01"), now).unwrap();
    assert_eq!(end.to_rfc3339(), "2024-03-01T15:00:00+00:00");
    assert!(!closed);
}

#[test]
fn a_retry_preserves_complete_sources_and_distinct_endpoints() {
    let owner = IdentityTarget::User {
        identity: identity("blue"),
    };
    let original = measurement(owner.clone(), "2024-02-29", 0);
    let mut other = original.clone();
    other.scope.endpoint = Uuid::new_v4();
    let mut failed_retry = measurement(owner, "2024-02-29", 10);
    failed_retry.measurement.storage = Reading::missing(Coverage::Denied, "Denied");
    failed_retry.measurement.traffic = Reading::new(
        Coverage::Partial,
        Some(Traffic::default()),
        Some("Partial".into()),
    );
    let mut document = Document::default();
    merge_measurements(&mut document, vec![other, original]);
    merge_measurements(&mut document, vec![failed_retry]);
    assert_eq!(document.measurements.len(), 2);
    let retained = document
        .measurements
        .iter()
        .find(|row| row.scope.endpoint.is_nil())
        .unwrap();
    assert_eq!(retained.measurement.storage.value, Some(0));
    assert_eq!(
        retained
            .measurement
            .traffic
            .value
            .as_ref()
            .unwrap()
            .requests,
        Some(0)
    );
}

#[test]
fn a_scoped_summary_keeps_one_verified_account_or_tenant_user() {
    let owner = identity("blue");
    let user = measurement(
        IdentityTarget::User {
            identity: owner.clone(),
        },
        "2024-02-29",
        10,
    );
    let another_tenant = measurement(
        IdentityTarget::User {
            identity: identity("red"),
        },
        "2024-02-29",
        30,
    );
    let account = measurement(
        IdentityTarget::Account {
            id: "RGW-one".into(),
        },
        "2024-02-29",
        50,
    );
    let rows = vec![user, another_tenant, account];
    let only_user = subject_measurements(rows.clone(), &owner, Some("RGW-one"), false);
    assert_eq!(only_user.len(), 1);
    assert_eq!(only_user[0].storage.value, Some(10));
    let only_account = subject_measurements(rows, &owner, Some("RGW-one"), true);
    assert_eq!(only_account.len(), 1);
    assert_eq!(only_account[0].storage.value, Some(50));
}

#[test]
fn pricing_accepts_zero_and_rejects_invalid_rates() {
    assert!(Rates::default().valid());
    assert!(
        !Rates {
            ingress: -1.0,
            ..Rates::default()
        }
        .valid()
    );
    assert!(
        !Rates {
            storage: f64::NAN,
            ..Rates::default()
        }
        .valid()
    );
}

#[tokio::test]
async fn historical_collection_rejects_cross_tenant_traffic_and_missing_storage() {
    use crate::administration::rgw_identities::{
        RgwIdentityService,
        model::{Registration, Registry},
        repository::RegistryRepository,
        tests::{fixture, gateway},
    };

    let today = Utc::now().date_naive();
    let closed_day = today.pred_opt().unwrap();
    let closed_day_start = format!("{closed_day} 00:00:00");
    let today_start = format!("{today} 00:00:00");
    let (url, worker) = gateway(2, move |method, path, fields| {
        assert_eq!(method, "GET");
        match path {
            "/admin/user" => {
                assert_eq!(fields.get("tenant").map(String::as_str), Some("blue"));
                assert_eq!(fields.get("uid").map(String::as_str), Some("blue$operator"));
                (200, r#"{"user_id":"operator","tenant":"blue","display_name":"Blue operator","stats":{"size":0}}"#.into())
            }
            "/admin/usage" => {
                assert_eq!(fields["uid"], "blue$operator");
                assert_eq!(fields["start"], closed_day_start);
                assert_eq!(fields["end"], today_start);
                (200, r#"{"summary":[{"user":"red$operator","total":{"ops":100,"bytes_sent":10,"bytes_received":5}}]}"#.into())
            }
            _ => panic!("Unexpected operation {path}"),
        }
    });
    let directory = tempfile::tempdir().unwrap();
    let (connections, _, id) = fixture(directory.path(), url.clone());
    let target = IdentityTarget::User {
        identity: identity("blue"),
    };
    RegistryRepository::new(directory.path().to_owned())
        .write(&Registry {
            identities: vec![Registration {
                scope: RegistryScope {
                    endpoint: id,
                    admin_url: url,
                },
                target: target.clone(),
                root: None,
            }],
            ..Registry::default()
        })
        .unwrap();
    let costs = super::StorageCosts::new(directory.path().to_owned());
    let identities = RgwIdentityService::new(directory.path().to_owned());
    let result = costs
        .collect(&connections, &identities, closed_day, Some(id))
        .await
        .unwrap();
    assert_eq!(result.subjects, 1);
    assert_eq!(result.traffic_records, 0);
    assert_eq!(result.storage_records, 0);
    assert_eq!(result.failures.len(), 2);
    let report = costs
        .report(
            &connections,
            closed_day.format("%Y-%m-01").to_string().parse().unwrap(),
        )
        .unwrap();
    assert_eq!(report.measurements.len(), 1);
    assert_eq!(report.measurements[0].subject, target);
    assert_eq!(report.measurements[0].traffic.coverage, Coverage::Missing);
    assert_eq!(report.measurements[0].storage.coverage, Coverage::Missing);
    worker.join().unwrap();
}

#[tokio::test]
async fn current_storage_only_collection_preserves_a_measured_zero() {
    use super::model::Settings;
    use crate::administration::rgw_identities::{
        RgwIdentityService,
        model::{Registration, Registry},
        repository::RegistryRepository,
        tests::{fixture, gateway},
    };

    let (url, worker) = gateway(1, |_, path, fields| {
        assert_eq!(path, "/admin/user");
        assert_eq!(fields["stats"], "true");
        (
            200,
            r#"{"user_id":"operator","tenant":"blue","stats":{"size":0}}"#.into(),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    let (connections, _, id) = fixture(directory.path(), url.clone());
    RegistryRepository::new(directory.path().to_owned())
        .write(&Registry {
            identities: vec![Registration {
                scope: RegistryScope {
                    endpoint: id,
                    admin_url: url,
                },
                target: IdentityTarget::User {
                    identity: identity("blue"),
                },
                root: None,
            }],
            ..Registry::default()
        })
        .unwrap();
    let costs = super::StorageCosts::new(directory.path().to_owned());
    costs
        .save(
            &connections,
            Settings {
                rates: Some(Rates::default()),
                collect_traffic: false,
                ..Settings::default()
            },
        )
        .unwrap();
    let today = Utc::now().date_naive();
    let result = costs
        .collect(
            &connections,
            &RgwIdentityService::new(directory.path().to_owned()),
            today,
            Some(id),
        )
        .await
        .unwrap();
    assert_eq!(result.storage_records, 1);
    assert!(result.failures.is_empty());
    let report = costs
        .report(
            &connections,
            today.format("%Y-%m-01").to_string().parse().unwrap(),
        )
        .unwrap();
    assert_eq!(report.measurements[0].storage.value, Some(0));
    assert_eq!(report.measurements[0].storage.coverage, Coverage::Complete);
    worker.join().unwrap();
}

#[tokio::test]
async fn summary_for_a_direct_profile_uses_the_registered_gateway_and_verifies_its_user() {
    use super::repository::CostRepository;
    use crate::{
        administration::rgw_identities::tests::{fixture, gateway},
        connection::repository::ConnectionRepository,
    };

    let (url, worker) = gateway(1, |_, path, fields| {
        assert_eq!(path, "/admin/user");
        assert_eq!(fields["access-key"], "connection-key");
        (200, r#"{"user_id":"operator","tenant":"blue","keys":[{"user":"blue$operator","access_key":"connection-key","secret_key":"fixture-secret"}]}"#.into())
    });
    let directory = tempfile::tempdir().unwrap();
    let (_, keyring, admin) = fixture(directory.path(), url.clone());
    let repository = ConnectionRepository::open(directory.path().to_owned()).unwrap();
    let direct_id = Uuid::new_v4();
    repository
        .update(|profiles| {
            let mut direct = profiles[0].clone();
            direct.profile.id = direct_id;
            direct.profile.settings.tenant = "blue".into();
            direct.profile.services.clear();
            direct.service_secret_ids.clear();
            profiles.push(direct);
            Ok(())
        })
        .unwrap();
    let connections =
        crate::connection::service::ConnectionService::with_credentials(repository, keyring);
    let target = IdentityTarget::User {
        identity: identity("blue"),
    };
    let mut owned = measurement(target.clone(), "2024-02-29", 22);
    owned.scope = RegistryScope {
        endpoint: admin,
        admin_url: url,
    };
    owned.measurement.endpoint_id = admin;
    let mut other = owned.clone();
    other.measurement.subject = IdentityTarget::User {
        identity: identity("red"),
    };
    CostRepository::new(directory.path().to_owned())
        .update(|document| {
            document.measurements = vec![owned, other];
        })
        .unwrap();
    let summary = super::StorageCosts::new(directory.path().to_owned())
        .summary(&connections, direct_id, date("2024-02-01"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(summary.measurements.len(), 1);
    assert_eq!(summary.measurements[0].subject, target);
    assert_eq!(summary.measurements[0].storage.value, Some(22));
    assert!(
        !serde_json::to_string(&summary)
            .unwrap()
            .contains("fixture-secret")
    );
    worker.join().unwrap();
}
