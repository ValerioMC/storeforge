use serde::Deserialize;

use crate::{
    common::error::{AppError, Result},
    connection::service::Session,
    monitoring::{
        collection::bucket_usage,
        model::{BucketUsage, Reading},
    },
    object::model::Location,
};

use super::{
    model::{Configuration, ConfigurationChange},
    repository,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationUpdate {
    pub configuration: Configuration,
    pub expected: String,
    pub change: ConfigurationChange,
}

pub async fn apply_configuration(
    session: &Session,
    bucket: &str,
    update: ConfigurationUpdate,
) -> Result<String> {
    let configuration = update.configuration;
    if !matches!(
        configuration,
        Configuration::Versioning
            | Configuration::Lifecycle
            | Configuration::Notification
            | Configuration::PublicAccessBlock
            | Configuration::Cors
            | Configuration::Policy
    ) || matches!(update.change, ConfigurationChange::Read)
    {
        return Err(AppError::Invalid(
            "Choose a supported campaign change.".into(),
        ));
    }
    if read_configuration(session, bucket, configuration).await? != update.expected {
        return Err(AppError::Invalid(
            "The provider configuration changed. Review a fresh preview before applying.".into(),
        ));
    }
    repository::configure(session, bucket, configuration, update.change).await?;
    read_configuration(session, bucket, configuration).await
}

async fn read_configuration(
    session: &Session,
    bucket: &str,
    configuration: Configuration,
) -> Result<String> {
    match repository::configure(session, bucket, configuration, ConfigurationChange::Read).await {
        Err(AppError::Missing(_)) => Ok(String::new()),
        result => result,
    }
}

pub async fn recalculate_usage(session: &Session, bucket: String) -> Reading<BucketUsage> {
    let location = Location {
        connection: session.context.connection_id,
        bucket,
        prefix: String::new(),
    };
    bucket_usage(&session.s3, &location, chrono::Utc::now()).await
}

#[cfg(test)]
#[path = "../../tests/unit/bucket/campaign.rs"]
mod bucket_campaigns;
