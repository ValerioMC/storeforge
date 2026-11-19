use super::{ProjectSession, route_segment};
use crate::{
    common::error::{AppError, Result},
    connection::shared_service::repository::SharedServiceRepository,
    portal::model::{AccessKey, AccessKeys, KeyReceipt, SharingChange, SharingRead},
};
use reqwest::Method;
use serde_json::{Value, json, to_value};

impl ProjectSession<'_> {
    pub(super) async fn read_sharing(&self, view: SharingRead) -> Result<Value> {
        match view {
            SharingRead::Requests => self.get("/api/portal/requests", &[]).await,
            SharingRead::Collaborators => self.get("/api/portal/collaborators", &[]).await,
            SharingRead::Collaborator { user_id } => {
                self.get(&format!("/api/portal/collaborators/{user_id}/access"), &[])
                    .await
            }
            SharingRead::Access => self.get(&self.space_path("/access-summary")?, &[]).await,
            SharingRead::Links => {
                self.get(
                    &self.space_path("/public-links")?,
                    &[("include_revoked", "true")],
                )
                .await
            }
            SharingRead::Keys => to_value(self.access_keys().await?).map_err(Into::into),
        }
    }

    pub(super) async fn change_sharing(&self, action: SharingChange) -> Result<Value> {
        match action {
            SharingChange::Audience { share_scope, account_member_role } => {
                self.update_space(Method::PATCH, "", &json!({ "share_scope": share_scope, "account_member_role": account_member_role })).await
            }
            SharingChange::Grant { user_id, role } => {
                self.update_space(Method::POST, "/shares", &json!({ "user_id": user_id, "role": role })).await
            }
            SharingChange::ChangeRole { user_id, role } => {
                self.update_space(Method::PUT, &format!("/shares/{user_id}"), &json!({ "role": role })).await
            }
            SharingChange::Revoke { user_id } => self.delete_record(&self.space_path(&format!("/shares/{user_id}"))?).await,
            SharingChange::CreateLink { object_key, label, expires_at } => {
                self.update_space(Method::POST, "/public-links", &json!({ "object_key": object_key, "label": label, "expires_at": expires_at })).await
            }
            SharingChange::RevokeLink { link_id } => self.delete_record(&self.space_path(&format!("/public-links/{link_id}"))?).await,
            SharingChange::PersonalKey => self.create_key(json!({ "target_type": "self" })).await,
            SharingChange::ExternalKey { storage_space_id, external_email, permission } => self.create_key(json!({
                "target_type": "external", "storage_space_id": storage_space_id, "external_email": external_email, "permission": permission,
            })).await,
            SharingChange::KeyStatus { key_id, active } => self.key_status(&key_id, active).await,
            SharingChange::DeleteKey { key_id } => self.delete_key(&key_id).await,
            SharingChange::AddMember { target_name, target_email, portal_role, reason } => self.membership_request(json!({
                "request_type": "portal_user_access", "intent": "add", "target_name": target_name, "target_email": target_email,
                "portal_role": portal_role, "reason": reason,
            })).await,
            SharingChange::RemoveMember { target_email, target_name, reason } => self.membership_request(json!({
                "request_type": "portal_user_removal", "target_email": target_email, "target_name": target_name, "reason": reason,
            })).await,
            SharingChange::MemberRole { target_user_id, portal_role, reason } => self.membership_request(json!({
                "request_type": "portal_user_access", "intent": "role_change", "target_user_id": target_user_id,
                "portal_role": portal_role, "reason": reason,
            })).await,
        }
    }

    async fn access_keys(&self) -> Result<AccessKeys> {
        let mut keys: AccessKeys = self.get("/api/portal/access-keys", &[]).await?;
        keys.access_keys.retain(|key| !key.is_portal);
        Ok(keys)
    }

    async fn create_key(&self, draft: Value) -> Result<Value> {
        let receipt: KeyReceipt = self
            .submit(Method::POST, "/api/portal/access-keys", &draft)
            .await?;
        if receipt.key.is_portal {
            return Err(AppError::AuthorizationDenied(
                "Runtime credentials are unavailable.".into(),
            ));
        }
        to_value(receipt).map_err(Into::into)
    }

    async fn key_status(&self, key_id: &str, active: bool) -> Result<Value> {
        let key_id = self.editable_key(key_id).await?;
        let status: AccessKey = self
            .submit(
                Method::PUT,
                &format!("/api/portal/access-keys/{key_id}/status"),
                &json!({ "active": active }),
            )
            .await?;
        if status.is_portal {
            return Err(AppError::AuthorizationDenied(
                "Runtime credentials are unavailable.".into(),
            ));
        }
        to_value(status).map_err(Into::into)
    }

    async fn delete_key(&self, key_id: &str) -> Result<Value> {
        let key_id = self.editable_key(key_id).await?;
        let request = self
            .portal
            .request(Method::DELETE, &format!("/api/portal/access-keys/{key_id}"))?
            .query(&[("account_id", &self.account)]);
        SharedServiceRepository::send(request).await?;
        Ok(Value::Null)
    }

    async fn editable_key(&self, key_id: &str) -> Result<String> {
        let route = route_segment(key_id)?;
        if !self
            .access_keys()
            .await?
            .access_keys
            .iter()
            .any(|key| key.access_key_id == key_id && key.deletable)
        {
            return Err(AppError::AuthorizationDenied(
                "This key cannot be changed in Portal.".into(),
            ));
        }
        Ok(route)
    }

    async fn delete_record(&self, path: &str) -> Result<Value> {
        let request = self
            .portal
            .request(Method::DELETE, path)?
            .query(&[("account_id", &self.account)]);
        SharedServiceRepository::json(request).await
    }

    async fn membership_request(&self, draft: Value) -> Result<Value> {
        self.submit(Method::POST, "/api/portal/requests", &draft)
            .await
    }
}
