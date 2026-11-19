use std::time::Duration;

use reqwest::{Method, StatusCode};
use serde_json::Value;

use crate::administration::repository::RgwClient;

use super::{model::*, project};

pub(super) async fn local_zone(client: &RgwClient<'_>) -> Observation<LocalZone> {
    observe(client, "config", [("type", "zone")], project::local_zone).await
}

pub(super) async fn realm(client: &RgwClient<'_>, realm: &str) -> Observation<Realm> {
    observe(client, "realm", [("id", realm)], |document| {
        project::realm(document, realm)
    })
    .await
}

pub(super) async fn period(client: &RgwClient<'_>, realm: &str) -> Observation<Period> {
    observe(
        client,
        "realm/period",
        [("realm_id", realm)],
        project::period,
    )
    .await
}

pub(super) async fn sync(
    client: &RgwClient<'_>,
    family: FlowFamily,
    source: &str,
) -> Observation<SyncStatus> {
    let fields = match family {
        FlowFamily::Metadata => vec![("type", "metadata"), ("status", "")],
        FlowFamily::Data => vec![("type", "data"), ("status", ""), ("source-zone", source)],
    };
    observe(client, "log", fields, |document| {
        project::flow(document, family)
    })
    .await
}

pub(super) async fn bucket(
    client: &RgwClient<'_>,
    target: &BucketSelector,
) -> Observation<BucketIdentity> {
    let bucket = bucket_name(&target.tenant, &target.name);
    let identity = observe(
        client,
        "bucket",
        [("bucket", bucket.as_str()), ("stats", "true")],
        |document| project::bucket(document, target),
    )
    .await;
    if identity.data.as_ref().is_some_and(|current| {
        current.name != target.name
            || current.tenant != target.tenant
            || target
                .instance
                .as_ref()
                .is_some_and(|expected| current.instance != *expected)
    }) {
        return Observation::absent(Availability::Stale, identity.status);
    }
    identity
}

pub(super) async fn replication(
    client: &RgwClient<'_>,
    source: &BucketIdentity,
    destination: &BucketIdentity,
    zone: &str,
) -> Observation<SyncStatus> {
    let source = sync_bucket(source);
    let destination = sync_bucket(destination);
    observe(
        client,
        "log",
        [
            ("type", "bucket-index"),
            ("status", ""),
            ("bucket", &destination),
            ("source-bucket", &source),
            ("source-zone", zone),
            ("version", "2"),
        ],
        project::bucket_sync,
    )
    .await
}

pub(super) async fn bilog(
    client: &RgwClient<'_>,
    bucket: &BucketIdentity,
) -> Observation<BilogInfo> {
    let mut fields = bilog_scope(bucket, None);
    fields.push(("info", String::new()));
    observe(client, "log", fields, project::bilog).await
}

pub(super) async fn journal(
    client: &RgwClient<'_>,
    scope: &JournalScope,
    bucket: Option<BucketIdentity>,
) -> Observation<JournalPage> {
    let mut fields = match (&scope.journal, &bucket) {
        (Journal::Metadata { period, shard }, _) => vec![
            ("type", "metadata".into()),
            ("period", period.clone()),
            ("id", shard.to_string()),
        ],
        (Journal::Data { shard }, _) => vec![
            ("type", "data".into()),
            ("id", shard.to_string()),
            ("extra-info", "true".into()),
        ],
        (
            Journal::Bilog {
                generation, shard, ..
            },
            Some(bucket),
        ) => {
            let mut fields = bilog_scope(bucket, *shard);
            fields.extend([
                ("generation", generation.clone()),
                ("format-ver", "2".into()),
            ]);
            fields
        }
        (Journal::Bilog { .. }, None) => return Observation::absent(Availability::Missing, None),
    };
    fields.push(("max-entries", scope.limit.to_string()));
    if let Some(marker) = &scope.marker {
        fields.push(("marker", marker.clone()));
    }
    observe(client, "log", fields, |document| {
        let mut page = project::journal(
            document,
            bucket.is_some(),
            scope.marker.as_deref(),
            scope.limit.into(),
        )?;
        page.bucket = bucket;
        Some(page)
    })
    .await
}

fn bucket_name(tenant: &str, name: &str) -> String {
    if tenant.is_empty() {
        name.into()
    } else {
        format!("{tenant}/{name}")
    }
}

fn sync_bucket(bucket: &BucketIdentity) -> String {
    format!(
        "{}:{}",
        bucket_name(&bucket.tenant, &bucket.name),
        bucket.instance
    )
}

fn bilog_scope(bucket: &BucketIdentity, shard: Option<u32>) -> Vec<(&'static str, String)> {
    let mut instance = format!("{}:{}", bucket.name, bucket.instance);
    if let Some(shard) = shard {
        instance.push_str(&format!(":{shard}"));
    }
    let mut fields = vec![
        ("type", "bucket-index".into()),
        ("bucket", bucket.name.clone()),
        ("bucket-instance", instance),
    ];
    if !bucket.tenant.is_empty() {
        fields.push(("tenant", bucket.tenant.clone()));
    }
    fields
}

async fn observe<T>(
    client: &RgwClient<'_>,
    resource: &str,
    fields: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
    project: impl FnOnce(Value) -> Option<T>,
) -> Observation<T> {
    let request = client.response(Method::GET, resource, fields);
    let Ok(Ok((status, body))) = tokio::time::timeout(Duration::from_secs(8), request).await else {
        return Observation::absent(Availability::Unavailable, None);
    };
    match status {
        status if status.is_success() => {
            let value = serde_json::from_str(&body).ok().and_then(project);
            match value {
                Some(value) => Observation::available(value, status.as_u16()),
                None => Observation::absent(Availability::Unavailable, Some(status.as_u16())),
            }
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            Observation::absent(Availability::Denied, Some(status.as_u16()))
        }
        StatusCode::NOT_FOUND => Observation::absent(Availability::Missing, Some(status.as_u16())),
        StatusCode::NOT_IMPLEMENTED | StatusCode::METHOD_NOT_ALLOWED => {
            Observation::absent(Availability::Unsupported, Some(status.as_u16()))
        }
        _ => Observation::absent(Availability::Unavailable, Some(status.as_u16())),
    }
}
