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
    model::{AdminRequest, Operation, Protocol},
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
        let result = repository::execute(session, operation, &values).await;
        tracing::info!(operation = %operation.label, connection = %session.context.connection_id, principal = %session.context.principal_id, success = result.is_ok(), "Administration request completed");
        audit.record(session.profile.id, &operation.label, None, result)
    }
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
        .filter(|(_, value)| !value.is_empty())
        .collect())
}

#[cfg(test)]
#[path = "../../tests/unit/administration/service.rs"]
mod tests;
