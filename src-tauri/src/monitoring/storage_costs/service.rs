use std::{collections::BTreeMap, path::PathBuf};

use chrono::{Datelike, Months, NaiveDate, Utc};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    administration::rgw_identities::{
        RgwIdentityService,
        model::{IdentityTarget, RegistryScope, UserIdentity},
        repository as identities,
    },
    common::error::{AppError, Result},
    connection::{
        model::{CapabilityState, Connection, IdentityKind, Workspace},
        service::{ConnectionService, Session},
    },
};

use super::super::model::{Coverage, Reading};
use super::{
    collection::{measure, source_error},
    model::{
        CollectionResult, Endpoint, Failure, Measurement, Report, ScopedMeasurement, Settings,
        Source, Summary,
    },
    repository::{CostRepository, merge_measurements},
};

pub struct StorageCosts {
    repository: CostRepository,
    collection: Mutex<()>,
}

impl StorageCosts {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            repository: CostRepository::new(directory),
            collection: Mutex::new(()),
        }
    }

    pub fn report(&self, connections: &ConnectionService, month: NaiveDate) -> Result<Report> {
        let range = report_range(month)?;
        let candidates = endpoints(connections)?;
        let document = self.repository.read()?;
        let measurements = document
            .measurements
            .into_iter()
            .filter_map(|row| {
                let endpoint = candidates.get(&row.scope.endpoint)?;
                (endpoint.scope.as_ref() == Some(&row.scope)
                    && range.contains(&row.measurement.day))
                .then_some(row.measurement)
            })
            .collect();
        let mut settings = document.settings;
        settings
            .overrides
            .retain(|id, _| candidates.contains_key(id));
        Ok(Report {
            settings,
            endpoints: candidates
                .into_iter()
                .map(|(id, entry)| Endpoint {
                    id,
                    name: entry.profile.name,
                })
                .collect(),
            measurements,
        })
    }

    pub fn save(&self, connections: &ConnectionService, mut settings: Settings) -> Result<()> {
        settings.currency = settings.currency.trim().to_ascii_uppercase();
        if settings.currency.len() != 3
            || !settings
                .currency
                .bytes()
                .all(|byte| byte.is_ascii_uppercase())
        {
            return Err(AppError::Invalid(
                "Enter a three-letter currency code.".into(),
            ));
        }
        if settings.rates.is_some_and(|rates| !rates.valid())
            || settings.overrides.values().any(|rates| !rates.valid())
        {
            return Err(AppError::Invalid(
                "Enter rates between 0 and 999999.999999.".into(),
            ));
        }
        let available = endpoints(connections)?;
        if settings
            .overrides
            .keys()
            .any(|id| !available.contains_key(id))
        {
            return Err(AppError::Invalid(
                "Select a registered cost endpoint for each rate override.".into(),
            ));
        }
        self.repository.update(|document| {
            settings.overrides.extend(
                document
                    .settings
                    .overrides
                    .iter()
                    .filter(|(id, _)| !available.contains_key(id))
                    .map(|(id, rate)| (*id, *rate)),
            );
            document.settings = settings;
        })
    }

    pub async fn summary(
        &self,
        connections: &ConnectionService,
        connection: Uuid,
        month: NaiveDate,
    ) -> Result<Option<Summary>> {
        let range = report_range(month)?;
        let profile = connections.with_profile(connection, |profile| Ok(profile.clone()))?;
        let Some(admin_profile) = summary_administrator(connections, &profile)? else {
            return Ok(None);
        };
        let Some(scope) =
            admin_scope(&admin_profile).filter(|scope| scope.endpoint == profile.endpoint_id())
        else {
            return Ok(None);
        };
        let document = self.repository.read()?;
        let scoped = document
            .measurements
            .into_iter()
            .filter(|row| row.scope == scope && range.contains(&row.measurement.day))
            .collect::<Vec<_>>();
        if scoped.is_empty() {
            return Ok(None);
        }
        let s3 = connections.session_for(connection, Workspace::Manager, IdentityKind::S3)?;
        let admin = connections.session_for(
            admin_profile.id,
            Workspace::CephAdmin,
            IdentityKind::AdminOps,
        )?;
        ensure_binding(&profile, &s3)?;
        ensure_binding(&admin_profile, &admin)?;
        if s3.credentials.session_token().is_some() {
            return Ok(None);
        }
        let owner = identities::owner(&admin, s3.credentials.access_key_id())
            .await
            .map_err(|error| {
                let reading = source_error::<()>(&error);
                AppError::Storage(
                    reading
                        .detail
                        .unwrap_or_else(|| "The cost scope could not be verified.".into()),
                )
            })?;
        if owner.identity.tenant != s3.context.tenant
            || owner.account.as_deref().unwrap_or_default() != s3.context.account
        {
            return Err(scope_changed());
        }
        if let Some(managed) = &profile.settings.managed_rgw
            && (owner.identity.uid != managed.uid
                || managed.access_key_id != s3.credentials.access_key_id())
        {
            return Err(scope_changed());
        }
        ensure_current(connections, &s3)?;
        ensure_current(connections, &admin)?;
        let measurements = subject_measurements(
            scoped,
            &owner.identity,
            owner.account.as_deref(),
            owner.account_root == Some(true),
        );
        Ok((!measurements.is_empty()).then(|| Summary {
            rates: document
                .settings
                .overrides
                .get(&scope.endpoint)
                .copied()
                .or(document.settings.rates),
            currency: document.settings.currency,
            measurements,
        }))
    }

    pub async fn collect(
        &self,
        connections: &ConnectionService,
        identities: &RgwIdentityService,
        day: NaiveDate,
        endpoint: Option<Uuid>,
    ) -> Result<CollectionResult> {
        let _collection = self
            .collection
            .try_lock()
            .map_err(|_| AppError::Invalid("Daily cost collection is already running.".into()))?;
        let now = Utc::now();
        if day > now.date_naive() || !(1..=9999).contains(&day.year()) {
            return Err(AppError::Invalid("Select a UTC day up to today.".into()));
        }
        let settings = self.repository.read()?.settings;
        if !settings.collect_storage && !settings.collect_traffic {
            return Err(AppError::Invalid(
                "Enable a collection source in Cost settings first.".into(),
            ));
        }
        let candidates = endpoints(connections)?;
        if endpoint.is_some_and(|id| !candidates.contains_key(&id)) {
            return Err(AppError::Invalid(
                "The selected cost endpoint is no longer registered.".into(),
            ));
        }
        let mut result = CollectionResult {
            day,
            subjects: 0,
            storage_records: 0,
            traffic_records: 0,
            failures: vec![],
        };
        let mut recorded = vec![];
        for (id, candidate) in candidates
            .into_iter()
            .filter(|(id, _)| endpoint.is_none_or(|filter| filter == *id))
        {
            match collect_endpoint(connections, identities, candidate, &settings, day).await {
                Ok(rows) => {
                    for row in &rows {
                        record_outcome(&mut result, &row.measurement, &settings);
                    }
                    recorded.extend(rows);
                }
                Err(error) => result.failures.push(Failure {
                    endpoint_id: id,
                    subject: None,
                    source: Source::Endpoint,
                    detail: error.to_string(),
                }),
            }
        }
        if !recorded.is_empty() {
            self.repository
                .update(|document| merge_measurements(document, recorded))?;
        }
        Ok(result)
    }
}

struct Candidate {
    profile: Connection,
    scope: Option<RegistryScope>,
}

fn endpoints(connections: &ConnectionService) -> Result<BTreeMap<Uuid, Candidate>> {
    let mut candidates: BTreeMap<_, Candidate> = BTreeMap::new();
    let mut profiles = connections.list()?;
    profiles.sort_by(|left, right| (&left.name, left.id).cmp(&(&right.name, right.id)));
    for profile in profiles {
        let Some(scope) = admin_scope(&profile) else {
            continue;
        };
        candidates
            .entry(scope.endpoint)
            .and_modify(|known| {
                if known.scope.as_ref() != Some(&scope) {
                    known.scope = None;
                }
            })
            .or_insert(Candidate {
                profile,
                scope: Some(scope),
            });
    }
    Ok(candidates)
}

fn summary_administrator(
    connections: &ConnectionService,
    profile: &Connection,
) -> Result<Option<Connection>> {
    if let Some(owner) = &profile.settings.managed_rgw {
        return connections.with_profile(owner.administrator, |administrator| {
            Ok(Some(administrator.clone()))
        });
    }
    if admin_scope(profile).is_some() {
        return Ok(Some(profile.clone()));
    }
    Ok(endpoints(connections)?
        .remove(&profile.endpoint_id())
        .and_then(|entry| entry.scope.map(|_| entry.profile)))
}

fn admin_scope(profile: &Connection) -> Option<RegistryScope> {
    if profile.capability(IdentityKind::AdminOps) != CapabilityState::Detected {
        return None;
    }
    let url = profile
        .services
        .get(&IdentityKind::AdminOps)
        .map(|service| service.endpoint.as_str())
        .filter(|url| !url.is_empty())
        .unwrap_or(&profile.endpoint);
    Some(RegistryScope {
        endpoint: profile.endpoint_id(),
        admin_url: url.trim_end_matches('/').into(),
    })
}

async fn collect_endpoint(
    connections: &ConnectionService,
    identities: &RgwIdentityService,
    candidate: Candidate,
    settings: &Settings,
    day: NaiveDate,
) -> Result<Vec<ScopedMeasurement>> {
    let scope = candidate.scope.ok_or_else(|| {
        AppError::Invalid("Multiple Admin Ops origins are registered for this endpoint.".into())
    })?;
    let session = connections.session_for(
        candidate.profile.id,
        Workspace::CephAdmin,
        IdentityKind::AdminOps,
    )?;
    ensure_binding(&candidate.profile, &session)?;
    if scope.admin_url != session.profile.endpoint.trim_end_matches('/') {
        return Err(scope_changed());
    }
    let registrations = identities.cost_registrations(&session)?;
    if registrations.is_empty() {
        return Err(AppError::Missing(
            "This endpoint has no managed RGW accounts or users.".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let mut measurements = Vec::new();
    for registration in registrations {
        if seen.insert(registration.target.clone()) {
            measurements.push(measure(connections, &session, registration, settings, day).await);
        }
    }
    ensure_current(connections, &session)?;
    Ok(measurements)
}

fn record_outcome(result: &mut CollectionResult, measurement: &Measurement, settings: &Settings) {
    result.subjects += 1;
    result.storage_records += usize::from(measurement.storage.value.is_some());
    result.traffic_records += usize::from(measurement.traffic.value.is_some());
    if settings.collect_storage {
        record_failure(result, measurement, Source::Storage, &measurement.storage);
    }
    if settings.collect_traffic {
        record_failure(result, measurement, Source::Traffic, &measurement.traffic);
    }
}

fn record_failure<T>(
    result: &mut CollectionResult,
    measurement: &Measurement,
    source: Source,
    reading: &Reading<T>,
) {
    if reading.coverage != Coverage::Complete {
        result.failures.push(Failure {
            endpoint_id: measurement.endpoint_id,
            subject: Some(measurement.subject.clone()),
            source,
            detail: reading
                .detail
                .clone()
                .unwrap_or_else(|| "The provider returned an incomplete measurement.".into()),
        });
    }
}

fn ensure_current(connections: &ConnectionService, session: &Session) -> Result<()> {
    connections.with_profile(session.profile.id, |profile| {
        ensure_binding(profile, session)
    })
}

fn ensure_binding(profile: &Connection, session: &Session) -> Result<()> {
    if profile.endpoint_id() != session.context.endpoint_id
        || profile.revision != session.profile.revision
        || profile.principal_id(session.context.identity) != Some(session.context.principal_id)
    {
        return Err(scope_changed());
    }
    Ok(())
}

fn scope_changed() -> AppError {
    AppError::Invalid("The saved connection does not match the cost measurement scope.".into())
}

pub(super) fn report_range(month: NaiveDate) -> Result<std::ops::Range<NaiveDate>> {
    if month.day() != 1 || !(1..=9999).contains(&month.year()) {
        return Err(AppError::Invalid("Select a valid UTC month.".into()));
    }
    let start = month
        .checked_sub_months(Months::new(11))
        .ok_or_else(scope_changed)?;
    let end = month
        .checked_add_months(Months::new(1))
        .ok_or_else(scope_changed)?;
    Ok(start..end)
}

pub(super) fn subject_measurements(
    rows: Vec<ScopedMeasurement>,
    owner: &UserIdentity,
    account: Option<&str>,
    account_root: bool,
) -> Vec<Measurement> {
    let user = IdentityTarget::User {
        identity: owner.clone(),
    };
    let account = account
        .filter(|_| account_root)
        .map(|id| IdentityTarget::Account { id: id.into() });
    let account_exists = account
        .as_ref()
        .is_some_and(|account| rows.iter().any(|row| &row.measurement.subject == account));
    let target = if account_exists {
        account.as_ref().unwrap_or(&user)
    } else {
        &user
    };
    rows.into_iter()
        .filter_map(|row| (&row.measurement.subject == target).then_some(row.measurement))
        .collect()
}
