use std::collections::{BTreeMap, HashSet};

use percent_encoding::percent_decode_str;
use reqwest::Method;
use serde::Deserialize;

use crate::{
    bucket::repository::configuration_url,
    common::{
        error::{AppError, Result},
        signing::{SignedClient, request},
    },
    connection::service::Session,
    object::model::{Location, ObjectId},
    transfer::inventory,
};

#[derive(Clone)]
pub struct Entry {
    pub object: ObjectId,
    pub marker: bool,
}

pub type Inventory = BTreeMap<String, Vec<Entry>>;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HistoryPage {
    is_truncated: bool,
    next_key_marker: Option<String>,
    next_version_id_marker: Option<String>,
    encoding_type: Option<String>,
    #[serde(rename = "$value", default)]
    entries: Vec<HistoryEntry>,
}

#[derive(Deserialize)]
enum HistoryEntry {
    Version(EntryFields),
    DeleteMarker(EntryFields),
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EntryFields {
    key: String,
    version_id: String,
    is_latest: bool,
}

pub async fn inventory(session: &Session, bucket: &str, history: bool) -> Result<Inventory> {
    if history {
        return version_inventory(session, bucket).await;
    }
    let location = Location {
        connection: session.profile.id,
        bucket: bucket.into(),
        prefix: String::new(),
    };
    let mut result = BTreeMap::new();
    for row in inventory::objects(&session.s3, &location).await? {
        let key = row.key;
        let entry = Entry {
            object: ObjectId {
                bucket: bucket.into(),
                key: key.clone(),
                version_id: None,
            },
            marker: false,
        };
        if key.is_empty() || result.insert(key, vec![entry]).is_some() {
            return Err(invalid_history());
        }
    }
    Ok(result)
}

async fn version_inventory(session: &Session, bucket: &str) -> Result<Inventory> {
    let client = SignedClient::for_connection(&session.profile)?;
    let mut result = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut cursor: Option<(String, Option<String>)> = None;
    loop {
        let mut url = configuration_url(session, bucket, "versions")?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("max-keys", "250")
                .append_pair("encoding-type", "url");
            if let Some((key, version)) = &cursor {
                query.append_pair("key-marker", key);
                if let Some(version) = version {
                    query.append_pair("version-id-marker", version);
                }
            }
        }
        let response = client
            .send(session, "s3", request(Method::GET, url, None, Vec::new())?)
            .await
            .map_err(|_| AppError::Storage("Could not list ordered object versions.".into()))?;
        cursor = append_page(&response, bucket, &mut result, &mut seen)?;
        if cursor.is_none() {
            for entries in result.values_mut() {
                entries.reverse();
            }
            return Ok(result);
        }
    }
}

fn append_page(
    document: &str,
    bucket: &str,
    inventory: &mut Inventory,
    seen: &mut HashSet<(String, Option<String>)>,
) -> Result<Option<(String, Option<String>)>> {
    let page = quick_xml::de::from_str::<HistoryPage>(document).map_err(|_| invalid_history())?;
    let decode = page.encoding_type.as_deref() == Some("url");
    for entry in page.entries {
        let (fields, marker) = match entry {
            HistoryEntry::Version(fields) => (fields, false),
            HistoryEntry::DeleteMarker(fields) => (fields, true),
            HistoryEntry::Other => continue,
        };
        let key = decoded_key(fields.key, decode)?;
        let entries = inventory.entry(key.clone()).or_default();
        if key.is_empty()
            || fields.version_id.is_empty()
            || fields.is_latest != entries.is_empty()
            || entries
                .iter()
                .any(|entry| entry.object.version_id.as_ref() == Some(&fields.version_id))
        {
            return Err(invalid_history());
        }
        entries.push(Entry {
            object: ObjectId {
                bucket: bucket.into(),
                key,
                version_id: Some(fields.version_id),
            },
            marker,
        });
    }
    if !page.is_truncated {
        return Ok(None);
    }
    let next = page
        .next_key_marker
        .filter(|key| !key.is_empty())
        .ok_or_else(invalid_history)?;
    let next = (decoded_key(next, decode)?, page.next_version_id_marker);
    if !seen.insert(next.clone()) {
        return Err(invalid_history());
    }
    Ok(Some(next))
}

fn decoded_key(value: String, encoded: bool) -> Result<String> {
    if !encoded {
        return Ok(value);
    }
    percent_decode_str(&value)
        .decode_utf8()
        .map(|decoded| decoded.into_owned())
        .map_err(|_| invalid_history())
}

fn invalid_history() -> AppError {
    AppError::Storage(
        "The provider returned an incomplete or inconsistent ordered inventory.".into(),
    )
}
