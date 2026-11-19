use super::*;
use crate::{
    connection::{
        model::{Connection, Secret},
        service::{describe_context, open_session},
    },
    object::service::tests::{object_drawer_client, object_drawer_mock},
};

fn fixture(endpoint: &str, source: InventorySource) -> Session {
    let profile: Connection = serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(), "revision": Uuid::new_v4(), "name": "Fixture", "provider": "ceph",
        "endpoint": endpoint, "region": "us-east-1", "pathStyle": true, "knownBuckets": [],
        "settings": { "tenant": "tenant-a" },
        "services": { "cephAdmin": { "principalId": Uuid::new_v4(), "endpoint": endpoint,
            "enabled": true, "managedExternally": false, "capability": "detected" } }
    }))
    .unwrap();
    let context = describe_context(&profile, source.workspace(), source.identity()).unwrap();
    let mut session = open_session(
        profile,
        context,
        Secret {
            access_key: "fixture-key".into(),
            secret_key: "fixture-secret".into(),
            session_token: None,
        },
    )
    .unwrap();
    session.s3 = object_drawer_client(endpoint);
    session
}

fn response(body: &str) -> (&'static str, String, Vec<u8>) {
    (
        "200 OK",
        format!("Content-Length: {}\r\n", body.len()),
        body.as_bytes().to_vec(),
    )
}

#[tokio::test]
async fn keeps_equal_rgw_names_in_distinct_tenants_and_projects_usage() {
    let (endpoint, requests) = object_drawer_mock(vec![response(
        r#"[
        {"bucket":"records","tenant":"tenant-a","id":"instance-1","owner":"first",
          "usage":{"rgw.main":{"size":250,"num_objects":2}},
          "bucket_quota":{"enabled":true,"max_size_kb":4,"max_objects":-1}},
        {"bucket":"records","tenant":"tenant-b","id":"instance-2","owner":"second","usage":{}},
        "tenant-c/records"
    ]"#,
    )]);
    let session = fixture(&endpoint, InventorySource::CephAdmin);
    let mut rows = Vec::new();
    list(&session, InventorySource::CephAdmin, |page| {
        rows.extend(page);
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].quota.as_ref().unwrap().max_bytes, "4096");
    assert_eq!(rows[0].bytes, Some(250));
    assert_eq!(rows[1].tenant, "tenant-b");
    assert_eq!(rows[1].instance.as_deref(), Some("instance-2"));
    assert_eq!(rows[1].objects, Some(0));
    assert_eq!(rows[2].tenant, "tenant-c");
    assert!(rows[2].instance.is_none());
    let request = &requests.join().unwrap()[0];
    assert!(request.starts_with("GET /admin/bucket?format=json&stats=true&max-entries=200 "));
    assert!(request.to_ascii_lowercase().contains("authorization:"));
}

#[tokio::test]
async fn refuses_cross_tenant_stale_profile_and_recreated_ceph_bucket() {
    let (endpoint, requests) = object_drawer_mock(vec![response(
        r#"{"bucket":"records","tenant":"tenant-a","id":"replacement"}"#,
    )]);
    let session = fixture(&endpoint, InventorySource::CephAdmin);
    let mut target = BucketTarget {
        connection: session.profile.id,
        revision: None,
        source: InventorySource::CephAdmin,
        tenant: "tenant-a".into(),
        name: "records".into(),
        instance: Some("old".into()),
    };
    assert!(
        validate_instance(&session, &target)
            .await
            .unwrap_err()
            .to_string()
            .contains("connection changed")
    );
    target.revision = session.profile.revision;
    target.tenant = "tenant-b".into();
    assert!(
        validate_instance(&session, &target)
            .await
            .unwrap_err()
            .to_string()
            .contains("outside")
    );
    target.tenant = "tenant-a".into();
    assert!(
        validate_instance(&session, &target)
            .await
            .unwrap_err()
            .to_string()
            .contains("instance changed")
    );
    let received = requests.join().unwrap();
    assert_eq!(received.len(), 1);
    assert!(received[0].contains("bucket=tenant-a%2Frecords"));
}

#[tokio::test]
async fn streams_standard_inventory_through_the_s3_client() {
    let body = r#"<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Buckets><Bucket><Name>records</Name></Bucket></Buckets></ListAllMyBucketsResult>"#;
    let (endpoint, requests) = object_drawer_mock(vec![response(body)]);
    let session = fixture(&endpoint, InventorySource::StorageOps);
    let mut pages = Vec::new();
    list(&session, InventorySource::StorageOps, |page| {
        pages.push(page);
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(pages[0][0].name, "records");
    assert_eq!(pages[0][0].tenant, "tenant-a");
    assert!(pages[0][0].instance.is_none());
    let received = requests.join().unwrap();
    let request = received[0].lines().next().unwrap();
    let mut parts = request.split_whitespace();
    assert_eq!(parts.next(), Some("GET"));
    assert_eq!(
        parts.next().unwrap().parse::<http::Uri>().unwrap().path(),
        "/"
    );
}

#[tokio::test]
async fn streams_each_rgw_page_and_follows_the_provider_marker() {
    let (endpoint, requests) = object_drawer_mock(vec![
        response(
            r#"{"buckets":["tenant-a/records"],"count":1,"truncated":true,"marker":"tenant-a/records"}"#,
        ),
        response(r#"{"buckets":["tenant-b/records"],"count":1,"truncated":false}"#),
    ]);
    let session = fixture(&endpoint, InventorySource::CephAdmin);
    let mut pages = Vec::new();
    list(&session, InventorySource::CephAdmin, |page| {
        pages.push(page);
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0][0].tenant, "tenant-a");
    assert_eq!(pages[1][0].tenant, "tenant-b");
    assert!(requests.join().unwrap()[1].contains("marker=tenant-a%2Frecords"));
}
