use chrono::Utc;
use tokio::task::JoinSet;

use crate::{
    common::error::{AppError, Result},
    connection::{
        model::{CapabilityState, Connection, IdentityKind, Provider, Workspace},
        service::{ConnectionService, Session},
    },
};

use super::repository::RgwClient;
pub(crate) use model::*;
use repository as rgw;

mod model;
mod project;
mod repository;

const IDENTITY: IdentityKind = IdentityKind::CephAdmin;

pub(crate) async fn topology(
    connections: &ConnectionService,
    anchor: ProfileScope,
) -> Result<Topology> {
    let profiles = connections.list()?;
    let profile = profiles
        .iter()
        .find(|profile| profile.id == anchor.connection)
        .ok_or_else(|| {
            AppError::Missing("The selected Ceph profile is no longer registered.".into())
        })?;
    let anchor_session = scoped_session(connections, anchor)?;
    let (anchor_registration, realm, period) = anchor_topology(profile, &anchor_session).await;
    let mut registrations = vec![anchor_registration];
    if period.data.is_some() {
        let peers = profiles
            .into_iter()
            .filter(|profile| profile.provider == Provider::Ceph && profile.id != anchor.connection)
            .collect::<Vec<_>>();
        for group in peers.chunks(6) {
            let mut pending = JoinSet::new();
            for profile in group {
                let scope = profile_scope(profile);
                let session = scoped_session(connections, scope)?;
                let name = profile.name.clone();
                pending.spawn(async move {
                    RegisteredZone {
                        scope,
                        name,
                        local: discover(&session).await,
                    }
                });
            }
            while let Some(outcome) = pending.join_next().await {
                registrations.push(outcome.map_err(|_| {
                    AppError::Storage("A gateway discovery task did not finish.".into())
                })?);
            }
        }
    }
    registrations.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.scope.connection.cmp(&right.scope.connection))
    });
    Ok(Topology {
        anchor,
        collected_at: Utc::now().to_rfc3339(),
        realm,
        period,
        registrations,
    })
}

async fn anchor_topology(
    profile: &Connection,
    session: &Observation<Session>,
) -> (RegisteredZone, Observation<Realm>, Observation<Period>) {
    let local = discover(session).await;
    let (realm, period) = match local.data.as_ref() {
        Some(zone) if zone.realm_id.is_some() => {
            let result = query_topology(session, zone).await;
            (result.0, result.1)
        }
        Some(_) => (
            Observation::absent(Availability::NotApplicable, None),
            Observation::absent(Availability::NotApplicable, None),
        ),
        None => (local.without_data(), local.without_data()),
    };
    (
        RegisteredZone {
            scope: profile_scope(profile),
            name: profile.name.clone(),
            local,
        },
        realm,
        period,
    )
}

async fn query_topology(
    session: &Observation<Session>,
    local: &LocalZone,
) -> (Observation<Realm>, Observation<Period>) {
    let (Some(session), Some(realm)) = (session.data.as_ref(), local.realm_id.as_deref()) else {
        return (session.without_data(), session.without_data());
    };
    let Ok(client) = RgwClient::new(session) else {
        return (
            Observation::absent(Availability::Unavailable, None),
            Observation::absent(Availability::Unavailable, None),
        );
    };
    let (realm, period) = tokio::join!(rgw::realm(&client, realm), rgw::period(&client, realm));
    if period.data.as_ref().is_some_and(|period| {
        !matches_period(local, period)
            || realm
                .data
                .as_ref()
                .and_then(|realm| realm.current_period.as_ref())
                .is_some_and(|id| *id != period.id)
    }) {
        return (
            realm,
            Observation::absent(Availability::Stale, period.status),
        );
    }
    (realm, period)
}

async fn discover(session: &Observation<Session>) -> Observation<LocalZone> {
    let Some(session) = &session.data else {
        return session.without_data();
    };
    match RgwClient::new(session) {
        Ok(client) => rgw::local_zone(&client).await,
        Err(_) => Observation::absent(Availability::Unavailable, None),
    }
}

pub(crate) async fn flow(
    connections: &ConnectionService,
    scope: FlowScope,
) -> Result<Observation<SyncStatus>> {
    let session = scoped_session(connections, scope.destination.scope)?;
    let Some(session) = session.data.as_ref() else {
        return Ok(session.without_data());
    };
    let client = RgwClient::new(session)?;
    let period = verify_zone(&client, &scope.destination).await;
    let Some(topology) = period.data.as_ref() else {
        return Ok(period.without_data());
    };
    let available = match scope.family {
        FlowFamily::Metadata => {
            topology.master_zone.as_deref() == Some(scope.source_zone.as_str())
                && scope.source_zone != scope.destination.zone
        }
        FlowFamily::Data => {
            permitted_data_flow(topology, &scope.source_zone, &scope.destination.zone)
        }
    };
    if !available {
        return Ok(Observation::absent(Availability::NotApplicable, None));
    }
    let sync = rgw::sync(&client, scope.family, &scope.source_zone).await;
    if sync
        .data
        .as_ref()
        .and_then(|sync| sync.period.as_deref())
        .is_some_and(|id| id != topology.id)
    {
        return Ok(Observation::absent(Availability::Stale, sync.status));
    }
    Ok(sync)
}

pub(crate) async fn replication(
    connections: &ConnectionService,
    scope: ReplicationScope,
) -> Result<Replication> {
    validate_bucket(&scope.source_bucket)?;
    validate_bucket(&scope.destination_bucket)?;
    if scope.source.zone == scope.destination.zone
        || scope.source.realm != scope.destination.realm
        || scope.source.period != scope.destination.period
        || scope.source.epoch != scope.destination.epoch
    {
        return Err(AppError::Invalid(
            "Select distinct source and destination zones from the same period.".into(),
        ));
    }
    let source = scoped_session(connections, scope.source.scope)?;
    let destination = scoped_session(connections, scope.destination.scope)?;
    let source_client = source.data.as_ref().map(RgwClient::new).transpose()?;
    let destination_client = destination.data.as_ref().map(RgwClient::new).transpose()?;
    let (source_bucket, destination_bucket) = tokio::join!(
        verified_bucket(
            source_client.as_ref(),
            &source,
            &scope.source,
            &scope.source_bucket
        ),
        verified_bucket(
            destination_client.as_ref(),
            &destination,
            &scope.destination,
            &scope.destination_bucket
        ),
    );
    let sync = async {
        match (
            source_bucket.data.as_ref(),
            destination_bucket.data.as_ref(),
            destination_client.as_ref(),
        ) {
            (Some(source), Some(destination), Some(client)) => {
                rgw::replication(client, source, destination, &scope.source.zone).await
            }
            _ => unavailable_bucket(&source_bucket, &destination_bucket),
        }
    };
    let bilog = async {
        match (source_bucket.data.as_ref(), source_client.as_ref()) {
            (Some(bucket), Some(client)) => rgw::bilog(client, bucket).await,
            _ => source_bucket.without_data(),
        }
    };
    let (sync, bilog) = tokio::join!(sync, bilog);
    Ok(Replication {
        source: source_bucket,
        destination: destination_bucket,
        sync,
        bilog,
    })
}

pub(crate) async fn journal(
    connections: &ConnectionService,
    scope: JournalScope,
) -> Result<Observation<JournalPage>> {
    validate_journal(&scope)?;
    let session = scoped_session(connections, scope.target.scope)?;
    let Some(session) = session.data.as_ref() else {
        return Ok(session.without_data());
    };
    read_journal(&RgwClient::new(session)?, &scope).await
}

async fn read_journal(
    client: &RgwClient<'_>,
    scope: &JournalScope,
) -> Result<Observation<JournalPage>> {
    let period = verify_zone(client, &scope.target).await;
    if period.data.is_none() {
        return Ok(period.without_data());
    }
    let bucket = if let Journal::Bilog { bucket, .. } = &scope.journal {
        let observed = rgw::bucket(client, bucket).await;
        if observed.data.is_none() {
            return Ok(observed.without_data());
        }
        observed.data
    } else {
        None
    };
    Ok(rgw::journal(client, scope, bucket).await)
}

async fn verified_bucket(
    client: Option<&RgwClient<'_>>,
    session: &Observation<Session>,
    binding: &ZoneBinding,
    target: &BucketSelector,
) -> Observation<BucketIdentity> {
    let Some(client) = client else {
        return session.without_data();
    };
    let verified = verify_zone(client, binding).await;
    if verified.data.is_none() {
        return verified.without_data();
    }
    rgw::bucket(client, target).await
}

async fn verify_zone(client: &RgwClient<'_>, binding: &ZoneBinding) -> Observation<Period> {
    let local = rgw::local_zone(client).await;
    let Some(zone) = &local.data else {
        return local.without_data();
    };
    if zone.id != binding.zone || zone.realm_id.as_deref() != Some(binding.realm.as_str()) {
        return Observation::absent(Availability::Stale, local.status);
    }
    let period = rgw::period(client, &binding.realm).await;
    if period.data.as_ref().is_some_and(|period| {
        period.id != binding.period
            || period.epoch != binding.epoch
            || !matches_period(zone, period)
    }) {
        return Observation::absent(Availability::Stale, period.status);
    }
    period
}

fn matches_period(zone: &LocalZone, period: &Period) -> bool {
    zone.realm_id.as_deref() == Some(period.realm_id.as_str())
        && period.zones.iter().any(|candidate| candidate.id == zone.id)
}

fn permitted_data_flow(period: &Period, source_id: &str, destination_id: &str) -> bool {
    let Some(source) = period.zones.iter().find(|zone| zone.id == source_id) else {
        return false;
    };
    let Some(destination) = period.zones.iter().find(|zone| zone.id == destination_id) else {
        return false;
    };
    source_id != destination_id
        && (destination.sync_from_all != Some(false)
            || destination.policy_configured
            || destination
                .sync_from
                .iter()
                .any(|entry| entry == source_id || entry == &source.name))
}

fn unavailable_bucket(
    source: &Observation<BucketIdentity>,
    destination: &Observation<BucketIdentity>,
) -> Observation<SyncStatus> {
    if source.data.is_none() {
        source.without_data()
    } else {
        destination.without_data()
    }
}

fn scoped_session(
    connections: &ConnectionService,
    scope: ProfileScope,
) -> Result<Observation<Session>> {
    let access = connections.with_profile(scope.connection, |profile| {
        Ok(
            match (
                profile.provider,
                profile.revision == scope.revision,
                profile.capability(IDENTITY),
            ) {
                (_, false, _) => Availability::Stale,
                (Provider::Ceph, true, CapabilityState::Detected) => Availability::Available,
                (Provider::Ceph, true, CapabilityState::Denied) => Availability::Denied,
                _ => Availability::Unsupported,
            },
        )
    });
    match access {
        Ok(Availability::Available) => {
            match connections.session_for(scope.connection, Workspace::CephAdmin, IDENTITY) {
                Ok(session) if session.profile.revision == scope.revision => Ok(Observation {
                    availability: Availability::Available,
                    status: None,
                    data: Some(session),
                }),
                Ok(_) => Ok(Observation::absent(Availability::Stale, None)),
                Err(error) => {
                    tracing::debug!(connection = %scope.connection, error_kind = ?std::mem::discriminant(&error), "Multisite session unavailable");
                    Ok(Observation::absent(Availability::Unavailable, None))
                }
            }
        }
        Ok(availability) => Ok(Observation::absent(availability, None)),
        Err(AppError::Missing(_)) => Ok(Observation::absent(Availability::Missing, None)),
        Err(error) => Err(error),
    }
}

fn profile_scope(profile: &Connection) -> ProfileScope {
    ProfileScope {
        connection: profile.id,
        revision: profile.revision,
    }
}

fn validate_bucket(target: &BucketSelector) -> Result<()> {
    if target.name.is_empty()
        || target.name.contains(['/', ':'])
        || target.tenant.contains(['/', ':'])
        || target
            .instance
            .as_deref()
            .is_some_and(|instance| instance.is_empty() || instance.contains(['/', ':']))
    {
        return Err(AppError::Invalid(
            "Enter the bucket name, tenant and instance in separate fields.".into(),
        ));
    }
    Ok(())
}

fn validate_journal(scope: &JournalScope) -> Result<()> {
    if !(1..=500).contains(&scope.limit)
        || scope
            .marker
            .as_ref()
            .is_some_and(|marker| marker.is_empty() || marker.len() > 16384)
    {
        return Err(AppError::Invalid(
            "The journal page size or marker is invalid.".into(),
        ));
    }
    match &scope.journal {
        Journal::Metadata { period, .. } if period.is_empty() => {
            Err(AppError::Invalid("Enter a metadata journal period.".into()))
        }
        Journal::Bilog {
            bucket, generation, ..
        } => {
            validate_bucket(bucket)?;
            if generation.parse::<u64>().is_err()
                || (scope.marker.is_some() && bucket.instance.is_none())
            {
                return Err(AppError::Invalid("Select an explicit generation and keep the verified bucket instance for pagination.".into()));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/administration/ceph_multisite.rs"]
mod ceph_multisite;
