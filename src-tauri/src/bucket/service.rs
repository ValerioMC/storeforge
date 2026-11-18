use aws_sdk_s3::types::{BucketLocationConstraint, CreateBucketConfiguration};

use crate::{
    common::error::{Result, storage_error},
    connection::{model::Provider, service::Session},
};

use super::model::Bucket;

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

    pub async fn create(session: &Session, bucket: &str, object_lock: bool) -> Result<()> {
        let mut request = session.s3.create_bucket().bucket(bucket);
        if object_lock {
            request = request.object_lock_enabled_for_bucket(true);
        }
        if session.profile.provider == Provider::Aws && session.profile.region != "us-east-1" {
            request = request.create_bucket_configuration(
                CreateBucketConfiguration::builder()
                    .location_constraint(BucketLocationConstraint::from(
                        session.profile.region.as_str(),
                    ))
                    .build(),
            );
        }
        request.send().await.map_err(storage_error)?;
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
