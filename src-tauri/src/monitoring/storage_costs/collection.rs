use chrono::{DateTime, Days, NaiveDate, Utc};

use crate::{
    administration::rgw_identities::{
        model::{IdentityTarget, Registration, UserIdentity},
        repository,
    },
    bucket::service::BucketService,
    common::error::{AppError, Result},
    connection::{
        model::{IdentityKind, Workspace},
        service::{ConnectionService, Session},
    },
    object::model::Location,
};

use super::{
    super::{
        collection::{bucket_usage, error_coverage},
        model::{Coverage, Reading, Traffic},
        rgw,
    },
    model::{Measurement, ScopedMeasurement, Settings},
};

pub(super) async fn measure(
    connections: &ConnectionService,
    session: &Session,
    registration: Registration,
    settings: &Settings,
    day: NaiveDate,
) -> ScopedMeasurement {
    let now = Utc::now();
    let target = registration.target;
    let (name, account_id, storage) =
        metadata(connections, session, &target, settings, day, now).await;
    let storage =
        if settings.collect_storage && day == now.date_naive() && day != Utc::now().date_naive() {
            Reading::missing(
                Coverage::Missing,
                "The storage scan crossed the UTC day boundary.",
            )
        } else {
            storage
        };
    let traffic = daily_traffic(session, &target, settings, day, Utc::now()).await;
    ScopedMeasurement {
        scope: registration.scope,
        measurement: Measurement {
            day,
            collected_at: Utc::now(),
            endpoint_id: session.context.endpoint_id,
            subject: target,
            name,
            account_id,
            storage,
            traffic,
        },
    }
}

pub(super) fn subject_id(target: &IdentityTarget) -> String {
    match target {
        IdentityTarget::Account { id } => id.clone(),
        IdentityTarget::User { identity } => identity.qualified(),
    }
}

async fn metadata(
    connections: &ConnectionService,
    session: &Session,
    target: &IdentityTarget,
    settings: &Settings,
    day: NaiveDate,
    now: DateTime<Utc>,
) -> (String, Option<String>, Reading<u64>) {
    let collect_storage = settings.collect_storage && day == now.date_naive();
    match target {
        IdentityTarget::Account { id } => match repository::account(session, id).await {
            Ok(account) => {
                let storage = if collect_storage {
                    match account.usage.as_ref().and_then(|usage| usage.bytes) {
                        Some(bytes) => Reading::complete(bytes),
                        None => match repository::account_usage(session, &account).await {
                            Ok(usage) => storage_bytes(usage.bytes),
                            Err(error) => source_error(&error),
                        },
                    }
                } else {
                    skipped_storage(settings)
                };
                (name_or_id(account.name, id), Some(id.clone()), storage)
            }
            Err(error) => (
                id.clone(),
                Some(id.clone()),
                metadata_error(error, settings, collect_storage),
            ),
        },
        IdentityTarget::User { identity } => {
            match repository::user(session, identity, collect_storage).await {
                Ok(user) => {
                    let storage = if collect_storage {
                        match user.usage.and_then(|usage| usage.bytes) {
                            Some(bytes) => Reading::complete(bytes),
                            None if user.account.is_none() => {
                                match inventory_session(connections, session, identity) {
                                    Ok(Some(s3)) => inventory_storage(&s3, now).await,
                                    Ok(None) => storage_bytes(None),
                                    Err(error) => source_error(&error),
                                }
                            }
                            None => storage_bytes(None),
                        }
                    } else {
                        skipped_storage(settings)
                    };
                    (
                        name_or_id(user.display_name, &identity.qualified()),
                        user.account,
                        storage,
                    )
                }
                Err(error) => (
                    identity.qualified(),
                    None,
                    metadata_error(error, settings, collect_storage),
                ),
            }
        }
    }
}

fn metadata_error(error: AppError, settings: &Settings, collect_storage: bool) -> Reading<u64> {
    if collect_storage {
        source_error(&error)
    } else {
        skipped_storage(settings)
    }
}

pub(super) fn source_error<T>(error: &AppError) -> Reading<T> {
    let coverage = error_coverage(error);
    let detail = if coverage == Coverage::Denied {
        "The provider denied this measurement."
    } else {
        "The provider could not verify this measurement."
    };
    Reading::missing(coverage, detail)
}

fn name_or_id(name: String, id: &str) -> String {
    if name.trim().is_empty() {
        id.into()
    } else {
        name
    }
}

fn skipped_storage(settings: &Settings) -> Reading<u64> {
    if settings.collect_storage {
        Reading::missing(
            Coverage::Missing,
            "Storage can only be measured on the current UTC day.",
        )
    } else {
        disabled()
    }
}

fn disabled<T>() -> Reading<T> {
    Reading::missing(
        Coverage::Unavailable,
        "Collection is disabled for this source.",
    )
}

fn storage_bytes(bytes: Option<u64>) -> Reading<u64> {
    bytes.map_or_else(
        || {
            Reading::missing(
                Coverage::Missing,
                "The gateway omitted scoped storage bytes.",
            )
        },
        Reading::complete,
    )
}

fn inventory_session(
    connections: &ConnectionService,
    admin: &Session,
    identity: &UserIdentity,
) -> Result<Option<Session>> {
    let profile = connections.list()?.into_iter().find(|profile| {
        profile.endpoint_id() == admin.context.endpoint_id
            && profile.known_buckets.is_empty()
            && profile.settings.managed_rgw.as_ref().is_some_and(|owner| {
                owner.administrator == admin.profile.id
                    && owner.tenant == identity.tenant
                    && owner.uid == identity.uid
                    && owner.account.is_empty()
            })
    });
    profile
        .map(|profile| connections.session_for(profile.id, Workspace::Manager, IdentityKind::S3))
        .transpose()
}

async fn inventory_storage(session: &Session, now: DateTime<Utc>) -> Reading<u64> {
    let buckets = match BucketService::list(session).await {
        Ok(buckets) => buckets,
        Err(error) => return source_error(&error),
    };
    let mut total = 0u64;
    let mut complete = true;
    let mut seen = buckets.is_empty();
    for bucket in buckets {
        let location = Location {
            connection: session.profile.id,
            bucket: bucket.name,
            prefix: String::new(),
        };
        let reading = bucket_usage(&session.s3, &location, now).await;
        complete &= reading.coverage == Coverage::Complete;
        if let Some(usage) = reading.value {
            seen |= usage.current_bytes.is_some() || usage.noncurrent_bytes.is_some();
            total = total
                .saturating_add(usage.current_bytes.unwrap_or(0))
                .saturating_add(usage.noncurrent_bytes.unwrap_or(0));
        }
    }
    if complete {
        Reading::complete(total)
    } else {
        Reading::new(
            Coverage::Partial,
            seen.then_some(total),
            Some("Some buckets or object versions could not be measured.".into()),
        )
    }
}

async fn daily_traffic(
    session: &Session,
    target: &IdentityTarget,
    settings: &Settings,
    day: NaiveDate,
    now: DateTime<Utc>,
) -> Reading<Traffic> {
    if !settings.collect_traffic {
        return disabled();
    }
    let Some((start, end, closed)) = traffic_window(day, now) else {
        return Reading::missing(
            Coverage::Missing,
            "No complete UTC hours have elapsed for this day.",
        );
    };
    let reading = rgw::traffic_for_period(session, start, end, Some(&subject_id(target))).await;
    let totals = reading.value.map(|period| period.totals);
    if closed
        && let Some(value) = &totals
        && value.bytes_sent.is_some()
        && value.bytes_received.is_some()
        && value.requests.is_some()
    {
        return Reading::complete(value.clone());
    }
    if !closed && totals.is_some() {
        return Reading::new(
            Coverage::Partial,
            totals,
            Some("Traffic covers completed hours of the current UTC day.".into()),
        );
    }
    match reading.coverage {
        Coverage::Denied => {
            Reading::missing(Coverage::Denied, "The gateway denied this traffic reading.")
        }
        Coverage::Unavailable => Reading::missing(
            Coverage::Unavailable,
            "The gateway could not provide this traffic reading.",
        ),
        _ => Reading::new(reading.coverage, totals, reading.detail),
    }
}

pub(super) fn traffic_window(
    day: NaiveDate,
    now: DateTime<Utc>,
) -> Option<(DateTime<Utc>, DateTime<Utc>, bool)> {
    let start = day.and_hms_opt(0, 0, 0)?.and_utc();
    let midnight = day
        .checked_add_days(Days::new(1))?
        .and_hms_opt(0, 0, 0)?
        .and_utc();
    let hour = DateTime::from_timestamp(now.timestamp().div_euclid(3600) * 3600, 0)?;
    let end = midnight.min(hour);
    (start < end).then_some((start, end, end == midnight))
}
