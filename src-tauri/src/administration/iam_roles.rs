use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::{
    common::error::{AppError, Result},
    connection::model::Provider,
};

#[derive(Deserialize)]
struct RoleTag {
    key: String,
    value: String,
}

pub(super) fn validate_iam_resource_write(
    action: &str,
    values: &BTreeMap<String, String>,
    provider: Provider,
) -> Result<()> {
    if action == "CreatePolicy" && provider == Provider::Ceph {
        return Err(AppError::Invalid(
            "Ceph RGW exposes provider-managed policies. Use an inline policy to define custom permissions.".into(),
        ));
    }
    if matches!(
        action,
        "CreatePolicyVersion" | "SetDefaultPolicyVersion" | "DeletePolicyVersion" | "DeletePolicy"
    ) && values
        .get("PolicyArn")
        .is_some_and(|arn| arn.split(':').nth(4) == Some("aws"))
    {
        return Err(AppError::Invalid(
            "Provider-managed policies are read-only.".into(),
        ));
    }
    if let Some(duration) = values.get("MaxSessionDuration")
        && !duration
            .parse::<u32>()
            .is_ok_and(|seconds| (3600..=43200).contains(&seconds))
    {
        return Err(AppError::Invalid(
            "Session duration must be between 3600 and 43200 seconds.".into(),
        ));
    }
    Ok(())
}

pub(super) fn role_tag_batches(
    mut values: BTreeMap<String, String>,
    provider: Provider,
) -> Result<Vec<BTreeMap<String, String>>> {
    let source = values
        .remove("Tags")
        .ok_or_else(|| AppError::Invalid("Enter role tags.".into()))?;
    let tags = serde_json::from_str::<Vec<RoleTag>>(&source).map_err(|_| {
        AppError::Invalid("Enter role tags as JSON objects with a key and value.".into())
    })?;
    if tags.is_empty() || tags.len() > 50 || tags.iter().any(|tag| tag.key.trim().is_empty()) {
        return Err(AppError::Invalid(
            "Enter between one and 50 named tags per request.".into(),
        ));
    }
    let mut keys = BTreeSet::new();
    if provider == Provider::Aws && tags.iter().any(|tag| !keys.insert(tag.key.to_lowercase())) {
        return Err(AppError::Invalid(
            "AWS IAM supports one value per role tag key.".into(),
        ));
    }
    let batch_size = if provider == Provider::Ceph { 9 } else { 50 };
    Ok(tags
        .chunks(batch_size)
        .map(|tags| encode_tags(&values, tags))
        .collect())
}

fn encode_tags(values: &BTreeMap<String, String>, tags: &[RoleTag]) -> BTreeMap<String, String> {
    let mut encoded = values.clone();
    for (position, tag) in tags.iter().enumerate() {
        let index = position + 1;
        encoded.insert(format!("Tags.member.{index}.Key"), tag.key.clone());
        encoded.insert(format!("Tags.member.{index}.Value"), tag.value.clone());
    }
    encoded
}

#[cfg(test)]
#[path = "../../tests/unit/administration/iam_roles.rs"]
mod tests;
