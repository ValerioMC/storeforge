use aws_sdk_s3::{
    Client,
    config::Region,
    types::{
        BucketLocationConstraint, BucketVersioningStatus, CreateBucketConfiguration,
        VersioningConfiguration,
    },
};

use crate::{
    common::error::{Result, storage_error},
    connection::{model::Provider, service::Session},
};

use super::model::{Bucket, BucketCreation};

pub struct BucketService;

impl BucketService {
    pub async fn list(session: &Session) -> Result<Vec<Bucket>> {
        if !session.profile.known_buckets.is_empty() {
            return Self::list_known(session).await;
        }
        if session.profile.provider != Provider::Aws {
            let page = session
                .s3
                .list_buckets()
                .send()
                .await
                .map_err(storage_error)?;
            return Ok(bucket_rows(page.buckets()));
        }
        let mut output = Vec::new();
        let mut pages = session.s3.list_buckets().into_paginator().send();
        while let Some(page) = pages.next().await {
            output.extend(bucket_rows(page.map_err(storage_error)?.buckets()));
        }
        output.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(output)
    }

    async fn list_known(session: &Session) -> Result<Vec<Bucket>> {
        let mut buckets = Vec::new();
        for name in &session.profile.known_buckets {
            session
                .s3
                .head_bucket()
                .bucket(name)
                .send()
                .await
                .map_err(storage_error)?;
            buckets.push(Bucket {
                name: name.clone(),
                created_at: None,
            });
        }
        Ok(buckets)
    }

    pub async fn create(
        session: &Session,
        bucket: &str,
        object_lock: bool,
        location: Option<&str>,
        versioning: bool,
    ) -> Result<BucketCreation> {
        let location = location.map(str::trim).filter(|value| !value.is_empty());
        let location = location.or_else(|| {
            (session.profile.provider == Provider::Aws).then_some(session.profile.region.as_str())
        });
        let client = creation_client(session, location);
        let mut request = client.create_bucket().bucket(bucket);
        if object_lock {
            request = request.object_lock_enabled_for_bucket(true);
        }
        if let Some(location) = location.filter(|location| {
            session.profile.provider != Provider::Aws || *location != "us-east-1"
        }) {
            request = request.create_bucket_configuration(
                CreateBucketConfiguration::builder()
                    .location_constraint(BucketLocationConstraint::from(location))
                    .build(),
            );
        }
        request.send().await.map_err(storage_error)?;
        let versioning_error = if versioning && !object_lock {
            Self::enable_versioning(&client, bucket)
                .await
                .err()
                .map(|error| error.to_string())
        } else {
            None
        };
        Ok(BucketCreation { versioning_error })
    }

    pub async fn finish_versioning(
        session: &Session,
        bucket: &str,
        location: Option<&str>,
    ) -> Result<()> {
        let client = creation_client(session, location);
        let current = client
            .get_bucket_versioning()
            .bucket(bucket)
            .send()
            .await
            .map_err(storage_error)?;
        if current.status() == Some(&BucketVersioningStatus::Enabled) {
            return Ok(());
        }
        Self::enable_versioning(&client, bucket).await
    }

    async fn enable_versioning(client: &Client, bucket: &str) -> Result<()> {
        client
            .put_bucket_versioning()
            .bucket(bucket)
            .versioning_configuration(
                VersioningConfiguration::builder()
                    .status(BucketVersioningStatus::Enabled)
                    .build(),
            )
            .send()
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    pub async fn delete(session: &Session, bucket: &str) -> Result<()> {
        session
            .s3
            .delete_bucket()
            .bucket(bucket)
            .send()
            .await
            .map_err(storage_error)?;
        Ok(())
    }
}

fn bucket_rows(buckets: &[aws_sdk_s3::types::Bucket]) -> Vec<Bucket> {
    let mut rows: Vec<_> = buckets
        .iter()
        .filter_map(|bucket| {
            Some(Bucket {
                name: bucket.name()?.into(),
                created_at: bucket.creation_date().map(ToString::to_string),
            })
        })
        .collect();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    rows
}

fn creation_client(session: &Session, location: Option<&str>) -> Client {
    if let Some(region) = location.filter(|region| {
        session.profile.provider == Provider::Aws && *region != session.profile.region
    }) {
        let config = session
            .s3
            .config()
            .to_builder()
            .region(Region::new(region.to_owned()));
        return Client::from_conf(config.build());
    }
    session.s3.clone()
}

#[cfg(test)]
#[path = "../../tests/unit/bucket/workspace.rs"]
mod bucket_workspace;
