use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    bucket::model::Configuration,
    common::error::{AppError, Result},
    connection::service::Session,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapping {
    pub source: String,
    pub target: String,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    PreCopy,
    Immediate,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Method {
    Streamed,
    Storage,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub source: Uuid,
    pub target: Uuid,
    pub mappings: Vec<Mapping>,
    pub mode: Mode,
    pub method: Method,
    pub history: bool,
    pub settings: bool,
    pub parallelism: usize,
    pub protect_destination: bool,
    pub grant_source_read: bool,
}

impl Draft {
    pub fn validate(&self, source: &Session, target: &Session) -> Result<()> {
        use std::collections::HashSet;
        let mut sources = HashSet::new();
        let mut targets = HashSet::new();
        if self.mappings.is_empty()
            || self.mappings.iter().any(|mapping| {
                mapping.source.is_empty()
                    || !destination_name(&mapping.target)
                    || !sources.insert(&mapping.source)
                    || !targets.insert(&mapping.target)
            })
        {
            return Err(AppError::Invalid(
                "Map each source once to a distinct, valid S3 destination name.".into(),
            ));
        }
        let same_namespace =
            same_endpoint(source, target) && source.context.tenant == target.context.tenant;
        if same_namespace && targets.iter().any(|target| sources.contains(target)) {
            return Err(AppError::Invalid(
                "A destination overlaps a source bucket on this endpoint and tenant.".into(),
            ));
        }
        if !(1..=32).contains(&self.parallelism) {
            return Err(AppError::Invalid(
                "Parallelism must be from 1 to 32.".into(),
            ));
        }
        if self.method == Method::Storage && !same_namespace {
            return Err(AppError::Invalid(
                "Storage-side copy requires connections on the same endpoint and tenant.".into(),
            ));
        }
        if self.grant_source_read && self.method != Method::Storage {
            return Err(AppError::Invalid(
                "A temporary source read grant requires storage-side copy.".into(),
            ));
        }
        Ok(())
    }
}

fn destination_name(name: &str) -> bool {
    let valid_end = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    (3..=63).contains(&name.len())
        && name.split('.').all(|label| {
            label.bytes().next().is_some_and(valid_end)
                && label.bytes().last().is_some_and(valid_end)
                && label.bytes().all(|byte| valid_end(byte) || byte == b'-')
        })
        && name.parse::<std::net::Ipv4Addr>().is_err()
        && !["xn--", "sthree-", "amzn-s3-demo-"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        && ![
            "-s3alias",
            "--ol-s3",
            ".mrap",
            "--x-s3",
            "--table-s3",
            "-an",
        ]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

pub fn same_endpoint(source: &Session, target: &Session) -> bool {
    source.context.endpoint_id == target.context.endpoint_id
        || (!source.profile.endpoint.is_empty()
            && source.profile.endpoint == target.profile.endpoint)
        || (source.profile.endpoint.is_empty()
            && target.profile.endpoint.is_empty()
            && source.profile.region == target.profile.region)
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    connection: Uuid,
    endpoint: Uuid,
    principal: Uuid,
    tenant: String,
    revision: Option<Uuid>,
    #[serde(default)]
    address: String,
    #[serde(default)]
    account: String,
    #[serde(default)]
    region: String,
}

impl From<&Session> for Binding {
    fn from(session: &Session) -> Self {
        Self {
            connection: session.profile.id,
            endpoint: session.context.endpoint_id,
            principal: session.context.principal_id,
            tenant: session.context.tenant.clone(),
            revision: session.profile.revision,
            address: session.profile.endpoint.clone(),
            account: session.context.account.clone(),
            region: session.context.region.clone(),
        }
    }
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Read,
    Active,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairPlan {
    pub mapping: Mapping,
    pub sample: Option<String>,
    pub settings_copied: Vec<Configuration>,
    pub settings_omitted: Vec<Configuration>,
    pub issues: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub revision: Uuid,
    pub phase: Phase,
    pub checked_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub pairs: Vec<PairPlan>,
}

impl Report {
    pub fn new(revision: Uuid, phase: Phase, pairs: Vec<PairPlan>) -> Self {
        let checked_at = Utc::now();
        Self {
            revision,
            phase,
            checked_at,
            expires_at: checked_at + Duration::minutes(15),
            pairs,
        }
    }

    pub fn passed(&self, revision: Uuid, phase: Phase) -> bool {
        self.revision == revision
            && self.phase == phase
            && self.expires_at > Utc::now()
            && self.pairs.iter().all(|plan| plan.issues.is_empty())
    }
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Source,
    Target,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Effect {
    Policy {
        side: Side,
        bucket: String,
        original: Option<serde_json::Value>,
        protection: serde_json::Value,
    },
    Probe {
        bucket: String,
        created: bool,
        upload: Option<String>,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Migration {
    #[serde(default)]
    pub run: Option<super::execution_model::Run>,
    pub id: Uuid,
    pub revision: Uuid,
    pub draft: Draft,
    pub updated_at: DateTime<Utc>,
    pub report: Option<Report>,
    pub checking: Option<Phase>,
    pub recovery_error: Option<String>,
    #[serde(flatten)]
    pub(crate) private: PrivateState,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateState {
    pub source_binding: Binding,
    pub target_binding: Binding,
    pub effects: Vec<Effect>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationView {
    run: Option<super::execution_model::RunView>,
    id: Uuid,
    revision: Uuid,
    draft: Draft,
    updated_at: DateTime<Utc>,
    report: Option<Report>,
    checking: Option<Phase>,
    recovery_required: bool,
    can_restore: bool,
    recovery_error: Option<String>,
}

impl Migration {
    pub fn new(id: Uuid, draft: Draft, source: &Session, target: &Session) -> Self {
        Self {
            run: None,
            id,
            revision: Uuid::new_v4(),
            draft,
            updated_at: Utc::now(),
            report: None,
            checking: None,
            recovery_error: None,
            private: PrivateState {
                source_binding: source.into(),
                target_binding: target.into(),
                effects: Vec::new(),
            },
        }
    }

    pub fn view(&self) -> MigrationView {
        MigrationView {
            run: self.run.as_ref().map(|run| run.view(&self.private.effects)),
            id: self.id,
            revision: self.revision,
            draft: self.draft.clone(),
            updated_at: self.updated_at,
            report: self.report.clone(),
            checking: self.checking,
            recovery_required: self.recovery_error.is_some()
                || (self.run.is_none()
                    && !self.private.effects.is_empty()
                    && self.checking.is_none()),
            recovery_error: self.recovery_error.clone(),
            can_restore: !self.private.effects.is_empty(),
        }
    }

    pub fn validate_bindings(&self, source: &Session, target: &Session) -> Result<()> {
        if self.private.source_binding != source.into()
            || self.private.target_binding != target.into()
        {
            return Err(AppError::Invalid(
                "A saved connection changed. Rebind the draft before running checks.".into(),
            ));
        }
        Ok(())
    }

    pub fn validate_recovery_scope(&self, source: &Session, target: &Session) -> Result<()> {
        let bindings_match = [
            (&self.private.source_binding, source),
            (&self.private.target_binding, target),
        ]
        .into_iter()
        .all(|(saved, session)| {
            let mut current = Binding::from(session);
            current.revision = saved.revision;
            current == *saved
        });
        if !bindings_match {
            return Err(AppError::Invalid("The migration's endpoint or identity changed. Restore the recorded connection scope first.".into()));
        }
        Ok(())
    }

    pub fn require_recovered(&self) -> Result<()> {
        if !self.private.effects.is_empty() {
            return Err(AppError::Invalid(
                "Restore access before continuing.".into(),
            ));
        }
        Ok(())
    }
}
