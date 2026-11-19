use std::collections::HashSet;

use serde_json::Value;

use super::model::*;

pub(super) fn local_zone(document: Value) -> Option<LocalZone> {
    let mut zone: LocalZone = serde_json::from_value(document).ok()?;
    if zone.id.is_empty() || zone.name.is_empty() {
        return None;
    }
    zone.realm_id = zone.realm_id.filter(|realm| !realm.is_empty());
    Some(zone)
}

pub(super) fn realm(document: Value, expected: &str) -> Option<Realm> {
    let realm: Realm = serde_json::from_value(document).ok()?;
    (realm.id == expected && !realm.name.is_empty()).then_some(realm)
}

pub(super) fn period(document: Value) -> Option<Period> {
    let mut zones = Vec::new();
    let mut ids = HashSet::new();
    for group in document.get("period_map")?.get("zonegroups")?.as_array()? {
        let group_id = field(group, "id")?;
        let group_name = field(group, "name")?;
        let policy_configured = group
            .get("sync_policy")
            .and_then(|policy| policy.get("groups"))
            .and_then(Value::as_array)
            .is_some_and(|groups| !groups.is_empty());
        for zone in group.get("zones")?.as_array()? {
            let id = field(zone, "id")?;
            if !ids.insert(id.clone()) {
                return None;
            }
            zones.push(Zone {
                id,
                name: field(zone, "name")?,
                group_id: group_id.clone(),
                group_name: group_name.clone(),
                read_only: zone.get("read_only").and_then(Value::as_bool),
                sync_from_all: zone.get("sync_from_all").and_then(Value::as_bool),
                sync_from: zone
                    .get("sync_from")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(text)
                    .collect(),
                policy_configured,
            });
        }
    }
    Some(Period {
        id: field(&document, "id")?,
        realm_id: field(&document, "realm_id")?,
        epoch: field(&document, "epoch"),
        realm_epoch: field(&document, "realm_epoch"),
        master_zone: field(&document, "master_zone").or_else(|| {
            document
                .get("period_map")
                .and_then(|map| field(map, "master_zone"))
        }),
        zones,
    })
}

pub(super) fn flow(document: Value, family: FlowFamily) -> Option<SyncStatus> {
    let info = document.get("info")?;
    let mut ids = HashSet::new();
    let shards = document
        .get("markers")?
        .as_array()?
        .iter()
        .map(|entry| {
            let id = unsigned(entry, "key")?.try_into().ok()?;
            if !ids.insert(id) {
                return None;
            }
            let values = entry.get("val")?;
            let phase = match family {
                FlowFamily::Metadata => match unsigned(values, "state") {
                    Some(0) => "full-sync".into(),
                    Some(1) => "incremental-sync".into(),
                    _ => "unknown".into(),
                },
                FlowFamily::Data => phase(field(values, "status")?, false),
            };
            Some(Shard {
                id,
                phase,
                marker: field(values, "marker"),
                timestamp: timestamp(values, "timestamp"),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(SyncStatus {
        phase: phase(field(info, "status")?, true),
        expected_shards: unsigned(info, "num_shards").and_then(|count| count.try_into().ok()),
        generation: None,
        period: field(info, "period"),
        shards,
    })
}

pub(super) fn bucket_sync(document: Value) -> Option<SyncStatus> {
    let info = document.get("sync_status")?;
    let shards = document
        .get("inc_status")?
        .as_array()?
        .iter()
        .enumerate()
        .map(|(id, entry)| {
            let marker = entry.get("inc_marker")?;
            Some(Shard {
                id: id.try_into().ok()?,
                phase: phase(field(entry, "status")?, false),
                marker: field(marker, "position"),
                timestamp: timestamp(marker, "timestamp"),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(SyncStatus {
        phase: phase(field(info, "state")?, false),
        expected_shards: None,
        generation: unsigned(info, "incremental_gen").map(|generation| generation.to_string()),
        period: None,
        shards,
    })
}

pub(super) fn bilog(document: Value) -> Option<BilogInfo> {
    let generations = document
        .get("generations")?
        .as_array()?
        .iter()
        .map(|entry| generation(entry, "gen"))
        .collect::<Option<Vec<_>>>()?;
    Some(BilogInfo {
        latest: unsigned(&document, "latest_gen").map(|number| number.to_string()),
        oldest: unsigned(&document, "oldest_gen").map(|number| number.to_string()),
        generations,
    })
}

pub(super) fn bucket(document: Value, expected: &BucketSelector) -> Option<BucketIdentity> {
    let name = field(&document, "bucket").unwrap_or_else(|| expected.name.clone());
    let tenant = field(&document, "tenant").unwrap_or_default();
    Some(BucketIdentity {
        name,
        tenant,
        instance: field(&document, "id")?,
    })
}

pub(super) fn journal(
    document: Value,
    bilog: bool,
    marker: Option<&str>,
    limit: usize,
) -> Option<JournalPage> {
    let rows = document.get("entries")?.as_array()?;
    if rows.len() > limit {
        return None;
    }
    let entries = rows.iter().map(journal_entry).collect::<Option<Vec<_>>>()?;
    let next_marker = if document.get("truncated")?.as_bool()? {
        let next = if bilog {
            rows.last()
                .and_then(|last| field(last, "op_id"))
                .or_else(|| field(&document, "marker"))
        } else {
            field(&document, "marker")
        }?;
        if Some(next.as_str()) == marker {
            return None;
        }
        Some(next)
    } else {
        None
    };
    let next_generation = match document.get("next_log") {
        Some(value) if !value.is_null() => Some(generation(value, "generation")?),
        _ => None,
    };
    Some(JournalPage {
        entries,
        next_marker,
        next_generation,
        last_update: timestamp(&document, "last_update"),
        bucket: None,
    })
}

fn journal_entry(row: &Value) -> Option<JournalEntry> {
    row.as_object()?;
    Some(JournalEntry {
        id: fields(row, &["op_id", "log_id", "id"]),
        key: fields(row, &["key", "object", "name"]),
        timestamp: timestamp(row, "timestamp").or_else(|| timestamp(row, "log_timestamp")),
        category: fields(row, &["section", "entity_type"]),
        operation: field(row, "op"),
        generation: unsigned(row, "gen")
            .or_else(|| unsigned(row, "generation"))
            .map(|number| number.to_string()),
        instance: field(row, "object_instance"),
    })
}

fn generation(row: &Value, key: &str) -> Option<Generation> {
    Some(Generation {
        generation: unsigned(row, key)?.to_string(),
        shards: unsigned(row, "num_shards")?.try_into().ok()?,
    })
}

fn phase(value: String, global: bool) -> String {
    match value.as_str() {
        "init" | "stopped" => value,
        "sync" | "building-full-sync-maps" if global => value,
        "full-sync" | "incremental-sync" if !global => value,
        _ => "unknown".into(),
    }
}

fn fields(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| field(value, key))
}

fn field(value: &Value, key: &str) -> Option<String> {
    text(value.get(key)?)
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Number(value) if value.is_i64() || value.is_u64() => Some(value.to_string()),
        _ => None,
    }
}

fn unsigned(value: &Value, key: &str) -> Option<u64> {
    let value = value.get(key)?;
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn timestamp(value: &Value, key: &str) -> Option<String> {
    field(value, key)
        .filter(|value| !value.starts_with("1970-01-01") && !value.starts_with("0000-00-00"))
}
