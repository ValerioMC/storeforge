use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::{
    common::{error::AppError, signing::endpoint_client},
    connection::{
        model::{Connection, IdentityKind},
        service::Session,
    },
};

use super::{
    collection::error_coverage,
    model::{CheckMode, Coverage, HealthSample, HealthStatus, Incident, Origin, Source},
};

const TIMEOUT: Duration = Duration::from_secs(8);
const INITIAL_SLOW_MS: u64 = 1_000;

pub async fn probe(
    profile: &Connection,
    mode: CheckMode,
    signed: Option<Result<Session, AppError>>,
) -> ProbeOutcome {
    let started = Instant::now();
    let result = match signed {
        Some(Ok(session)) => tokio::time::timeout(TIMEOUT, signed_status(&session)).await,
        Some(Err(error)) => return ProbeOutcome::ineligible(error),
        None => tokio::time::timeout(TIMEOUT, http_status(profile)).await,
    };
    let elapsed = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    match result {
        Ok(Ok(status)) => {
            let state = match status {
                200..=299 => HealthStatus::Up,
                401 | 403 | 404 | 405 if mode == CheckMode::Http => HealthStatus::Up,
                401 | 403 => HealthStatus::Denied,
                429 => HealthStatus::Slow,
                _ => HealthStatus::Down,
            };
            ProbeOutcome {
                state,
                latency_ms: Some(elapsed),
                http_status: Some(status),
                detail: None,
            }
        }
        Ok(Err(error)) => ProbeOutcome {
            state: if matches!(error, AppError::Invalid(_)) {
                HealthStatus::Unavailable
            } else {
                HealthStatus::Down
            },
            latency_ms: None,
            http_status: None,
            detail: Some(error.to_string()),
        },
        Err(_) => ProbeOutcome {
            state: HealthStatus::Down,
            latency_ms: None,
            http_status: None,
            detail: Some("The endpoint did not answer within 8 seconds.".into()),
        },
    }
}

async fn signed_status(session: &Session) -> Result<u16, AppError> {
    if let Some(bucket) = session.profile.known_buckets.first() {
        return match session.s3.head_bucket().bucket(bucket).send().await {
            Ok(_) => Ok(200),
            Err(error) => error
                .raw_response()
                .map(|response| response.status().as_u16())
                .ok_or_else(|| {
                    AppError::Storage(
                        "The signed bucket check could not reach the endpoint.".into(),
                    )
                }),
        };
    }
    match session.s3.list_buckets().send().await {
        Ok(_) => Ok(200),
        Err(error) => error
            .raw_response()
            .map(|response| response.status().as_u16())
            .ok_or_else(|| {
                AppError::Storage("The signed service check could not reach the endpoint.".into())
            }),
    }
}

async fn http_status(profile: &Connection) -> Result<u16, AppError> {
    if profile.endpoint.is_empty() {
        return Err(AppError::Invalid(
            "Select a signed check for the AWS regional endpoint.".into(),
        ));
    }
    endpoint_client(profile, TIMEOUT)?
        .head(&profile.endpoint)
        .send()
        .await
        .map(|response| response.status().as_u16())
        .map_err(|error| AppError::Storage(error.without_url().to_string()))
}

pub struct ProbeOutcome {
    pub state: HealthStatus,
    pub latency_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub detail: Option<String>,
}

impl ProbeOutcome {
    fn ineligible(error: AppError) -> Self {
        Self {
            state: if error_coverage(&error) == Coverage::Denied {
                HealthStatus::Denied
            } else {
                HealthStatus::Unavailable
            },
            latency_ms: None,
            http_status: None,
            detail: Some(error.to_string()),
        }
    }

    pub fn sample(
        self,
        profile: &Connection,
        mode: CheckMode,
        checked_at: DateTime<Utc>,
        previous: &[HealthSample],
    ) -> HealthSample {
        let baseline_ms = baseline(previous, mode);
        let threshold =
            baseline_ms.map_or(INITIAL_SLOW_MS, |value| value.saturating_mul(3).max(500));
        let status = if self.state == HealthStatus::Up
            && self.latency_ms.is_some_and(|latency| latency >= threshold)
        {
            HealthStatus::Slow
        } else {
            self.state
        };
        let origin = match mode {
            CheckMode::Http => Origin::new(profile, Source::Http, None),
            CheckMode::Signed => Origin::new(profile, Source::SignedS3, Some(IdentityKind::S3)),
        };
        HealthSample {
            checked_at,
            origin,
            mode,
            status,
            latency_ms: self.latency_ms,
            baseline_ms,
            http_status: self.http_status,
            detail: self.detail,
        }
    }
}

fn baseline(samples: &[HealthSample], mode: CheckMode) -> Option<u64> {
    let mut values = samples
        .iter()
        .rev()
        .filter(|sample| sample.mode == mode && sample.status == HealthStatus::Up)
        .filter_map(|sample| sample.latency_ms)
        .take(20)
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}

pub fn apply_incident(incidents: &mut Vec<Incident>, sample: &HealthSample) {
    let active = incidents
        .iter_mut()
        .rev()
        .find(|incident| incident.mode == sample.mode && incident.recovered_at.is_none());
    match (sample.status, active) {
        (HealthStatus::Up, Some(incident)) => incident.recovered_at = Some(sample.checked_at),
        (HealthStatus::Down, Some(incident)) => incident.severity = HealthStatus::Down,
        (HealthStatus::Slow | HealthStatus::Down, None) => incidents.push(Incident {
            mode: sample.mode,
            opened_at: sample.checked_at,
            recovered_at: None,
            severity: sample.status,
        }),
        _ => {}
    }
}
