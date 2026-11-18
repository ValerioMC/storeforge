use aws_sdk_s3::{
    Client,
    operation::head_object::HeadObjectOutput,
    types::{DeleteMarkerEntry, ObjectVersion as S3Version},
};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::common::error::{AppError, Result, storage_error};

use super::model::{
    ObjectId, ObjectPage, ObjectQuery, ObjectRow, ObjectVersion, VersionCursor, VersionPage,
};

const S3_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub async fn list(client: &Client, query: &ObjectQuery) -> Result<ObjectPage> {
    let page = client
        .list_objects_v2()
        .bucket(&query.location.bucket)
        .prefix(&query.location.prefix)
        .set_delimiter((!query.recursive).then(|| "/".into()))
        .set_continuation_token(query.cursor.clone())
        .max_keys(500)
        .send()
        .await
        .map_err(storage_error)?;
    let mut rows: Vec<_> = page
        .common_prefixes()
        .iter()
        .filter_map(|prefix| {
            Some(ObjectRow {
                key: prefix.prefix()?.into(),
                folder: true,
                size: None,
                modified: None,
                storage_class: None,
                etag: None,
            })
        })
        .collect();
    rows.extend(page.contents().iter().filter_map(|object| {
        let key = object.key()?;
        if !query.recursive && key.ends_with('/') {
            return None;
        }
        Some(ObjectRow {
            key: key.into(),
            folder: false,
            size: object.size(),
            modified: object.last_modified().map(ToString::to_string),
            storage_class: object.storage_class().map(ToString::to_string),
            etag: object.e_tag().map(str::to_owned),
        })
    }));
    Ok(ObjectPage {
        rows,
        cursor: page.next_continuation_token().map(str::to_owned),
    })
}

pub async fn head(client: &Client, object: &ObjectId) -> Result<HeadObjectOutput> {
    client
        .head_object()
        .bucket(&object.bucket)
        .key(&object.key)
        .set_version_id(object.version_id.clone())
        .send()
        .await
        .map_err(storage_error)
}

pub async fn ensure_absent(client: &Client, bucket: &str, key: &str) -> Result<()> {
    match client.head_object().bucket(bucket).key(key).send().await {
        Ok(_) => Err(AppError::Invalid(
            "The destination already contains this key.".into(),
        )),
        Err(error)
            if error
                .raw_response()
                .is_some_and(|response| response.status().as_u16() == 404) =>
        {
            Ok(())
        }
        Err(error) => Err(storage_error(error)),
    }
}

pub async fn versions(
    client: &Client,
    bucket: &str,
    prefix: &str,
    exact: bool,
    cursor: Option<VersionCursor>,
) -> Result<VersionPage> {
    let (key_marker, version_marker) =
        cursor.map_or((None, None), |cursor| (Some(cursor.key), cursor.version));
    let page = client
        .list_object_versions()
        .bucket(bucket)
        .prefix(prefix)
        .set_key_marker(key_marker)
        .set_version_id_marker(version_marker)
        .max_keys(250)
        .send()
        .await
        .map_err(storage_error)?;
    let mut versions: Vec<_> = page
        .versions()
        .iter()
        .filter_map(version_row)
        .chain(page.delete_markers().iter().filter_map(marker_row))
        .filter(|version| !exact || version.key == prefix)
        .collect();
    versions.sort_by(|left, right| right.modified.cmp(&left.modified));
    let cursor = page
        .next_key_marker()
        .filter(|key| !exact || *key == prefix)
        .map(|key| VersionCursor {
            key: key.into(),
            version: page.next_version_id_marker().map(str::to_owned),
        });
    Ok(VersionPage { versions, cursor })
}

fn version_row(version: &S3Version) -> Option<ObjectVersion> {
    Some(ObjectVersion {
        key: version.key()?.into(),
        version_id: version.version_id()?.into(),
        latest: version.is_latest().unwrap_or_default(),
        delete_marker: false,
        size: version.size(),
        modified: version.last_modified().map(ToString::to_string),
    })
}

fn marker_row(version: &DeleteMarkerEntry) -> Option<ObjectVersion> {
    Some(ObjectVersion {
        key: version.key()?.into(),
        version_id: version.version_id()?.into(),
        latest: version.is_latest().unwrap_or_default(),
        delete_marker: true,
        size: None,
        modified: version.last_modified().map(ToString::to_string),
    })
}

pub fn copy_source(object: &ObjectId) -> String {
    let key = object
        .key
        .split('/')
        .map(|part| utf8_percent_encode(part, S3_COMPONENT).to_string())
        .collect::<Vec<_>>()
        .join("/");
    let mut source = format!("{}/{key}", object.bucket);
    if let Some(version) = &object.version_id {
        source.push_str(&format!(
            "?versionId={}",
            utf8_percent_encode(version, S3_COMPONENT)
        ));
    }
    source
}

pub fn required_id(id: Option<&str>, resource: &str) -> Result<String> {
    id.map(str::to_owned)
        .ok_or_else(|| AppError::Storage(format!("The endpoint did not return a {resource} ID.")))
}

#[cfg(test)]
#[path = "../../tests/unit/object/repository.rs"]
mod tests;
