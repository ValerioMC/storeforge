use std::collections::BTreeMap;

use crate::{
    audit::service::AuditService,
    common::error::{AppError, Result},
    connection::{
        model::{IdentityKind, Provider, Workspace},
        service::Session,
    },
};

use super::{
    iam_roles::{role_tag_batches, validate_iam_resource_write},
    model::{AdminRequest, Operation, Protocol},
    private_access::ensure_generic_iam_write,
    repository,
};

pub struct AdministrationService {
    operations: Vec<Operation>,
}

impl AdministrationService {
    pub fn new() -> Result<Self> {
        let operations = serde_json::from_str(include_str!(
            "../../../src/features/administration/model/operations.json"
        ))?;
        Ok(Self { operations })
    }

    pub fn execution_identity(&self, id: &str) -> Result<(Workspace, IdentityKind)> {
        let operation = self.operation(id)?;
        if operation.protocol != Protocol::Rgw {
            return Ok((Workspace::Manager, IdentityKind::S3));
        }
        let identity = match operation.group.as_str() {
            "Ceph accounts" | "Ceph users" => IdentityKind::AdminOps,
            "Ceph monitoring" => IdentityKind::Supervision,
            _ => IdentityKind::CephAdmin,
        };
        Ok((Workspace::CephAdmin, identity))
    }

    fn operation(&self, id: &str) -> Result<&Operation> {
        self.operations
            .iter()
            .find(|operation| operation.id == id)
            .ok_or_else(|| {
                AppError::Invalid("Select an available administration operation.".into())
            })
    }

    pub(crate) async fn iam(
        &self,
        session: &Session,
        action: &str,
        values: BTreeMap<String, String>,
    ) -> Result<String> {
        let operation = self
            .operations
            .iter()
            .find(|operation| operation.protocol == Protocol::Iam && operation.action == action)
            .ok_or_else(|| AppError::Invalid("Unsupported IAM operation.".into()))?;
        let values = if action == "GetUser" && values.is_empty() {
            values
        } else {
            validate_request(operation, values, true, session.profile.provider)?
        };
        repository::execute(session, operation, &values).await
    }

    pub async fn execute(
        &self,
        session: &Session,
        request: AdminRequest,
        audit: &AuditService,
    ) -> Result<String> {
        let operation = self.operation(&request.operation)?;
        let values = validate_request(
            operation,
            request.values,
            request.confirmed,
            session.profile.provider,
        )?;
        if operation.protocol == Protocol::Iam && operation.write {
            ensure_generic_iam_write(self, session, &operation.action, &values).await?;
            validate_iam_resource_write(&operation.action, &values, session.profile.provider)?;
        }
        let result = if operation.action == "TagRole" {
            write_role_tags(session, operation, values).await
        } else {
            repository::execute(session, operation, &values).await
        };
        tracing::info!(operation = %operation.label, connection = %session.context.connection_id, principal = %session.context.principal_id, success = result.is_ok(), "Administration request completed");
        audit.record(session.profile.id, &operation.label, None, result)
    }
}

async fn write_role_tags(
    session: &Session,
    operation: &Operation,
    values: BTreeMap<String, String>,
) -> Result<String> {
    let mut response = String::new();
    for batch in role_tag_batches(values, session.profile.provider)? {
        response = repository::execute(session, operation, &batch).await?;
    }
    Ok(response)
}

fn validate_request(
    operation: &Operation,
    values: BTreeMap<String, String>,
    confirmed: bool,
    provider: Provider,
) -> Result<BTreeMap<String, String>> {
    if operation.protocol == Protocol::Rgw && provider != Provider::Ceph {
        return Err(AppError::Invalid(
            "Select a Ceph RGW connection with Admin Ops permissions.".into(),
        ));
    }
    if operation.write && !confirmed {
        return Err(AppError::Invalid(
            "Review and confirm this change first.".into(),
        ));
    }
    if let Some(label) = operation.fields.iter().find(|field| {
        field.required
            && !(operation.action == "SetTopicAttributes"
                && field.key == "AttributeValue"
                && values.contains_key(&field.key))
            && values
                .get(&field.key)
                .is_none_or(|value| value.trim().is_empty())
    }) {
        return Err(AppError::Invalid(format!("Enter {}.", label.label)));
    }
    if values
        .keys()
        .any(|key| operation.fields.iter().all(|field| &field.key != key))
    {
        return Err(AppError::Invalid(
            "The operation received an unknown field.".into(),
        ));
    }
    Ok(values
        .into_iter()
        .filter(|(key, value)| {
            !value.is_empty()
                || (operation.action == "SetTopicAttributes" && key == "AttributeValue")
        })
        .collect())
}

#[cfg(test)]
#[path = "../../tests/unit/administration/service.rs"]
mod tests;
