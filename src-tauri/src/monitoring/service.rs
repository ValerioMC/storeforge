use std::path::PathBuf;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    bucket::service::BucketService,
    common::error::{AppError, Result},
    connection::{
        model::{CapabilityState, Connection, IdentityKind, Workspace},
        service::{ConnectionService, Session},
    },
    object::model::Location,
};

use super::{
    collection::{bucket_usage, reading_error},
    health,
    model::{
        BucketSample, CheckMode, Coverage, MonitoringView, Origin, Reading, Scope, Source,
        UsageSample,
    },
    repository::MonitoringRepository,
    rgw,
};

pub struct MonitoringService {
    repository: MonitoringRepository,
}

impl MonitoringService {
    pub fn open(directory: PathBuf) -> Result<Self> {
        Ok(Self {
            repository: MonitoringRepository::open(directory)?,
        })
    }

    pub fn cached(&self, connections: &ConnectionService, id: Uuid) -> Result<MonitoringView> {
        connections.with_profile(id, |profile| {
            self.repository.view(profile.into(), Utc::now())
        })
    }

    pub async fn collect(
        &self,
        connections: &ConnectionService,
        id: Uuid,
    ) -> Result<MonitoringView> {
        let profile = connections.with_profile(id, |profile| Ok(profile.clone()))?;
        let scope = Scope::from(&profile);
        let now = Utc::now();
        let listing =
            collection_session(connections, &profile, Workspace::Manager, IdentityKind::S3);
        let quota = collection_session(
            connections,
            &profile,
            Workspace::CephAdmin,
            IdentityKind::CephAdmin,
        );
        let supervision = collection_session(
            connections,
            &profile,
            Workspace::CephAdmin,
            IdentityKind::Supervision,
        );
        let ((bucket_coverage, buckets), (traffic, scoped_traffic)) =
            tokio::join!(collect_buckets(&profile, &listing, &quota, now), async {
                match &supervision {
                    Ok(session) => tokio::join!(
                        rgw::traffic(session, now, None),
                        scoped_traffic(session, now)
                    ),
                    Err(reason) => (reason.reading(), reason.reading()),
                }
            },);
        let sample = UsageSample {
            collected_at: now,
            listing_origin: Origin::new(&profile, Source::S3Listing, Some(IdentityKind::S3)),
            quota_origin: Origin::new(&profile, Source::RgwBucket, Some(IdentityKind::CephAdmin)),
            traffic_origin: Origin::new(
                &profile,
                Source::RgwUsage,
                Some(IdentityKind::Supervision),
            ),
            bucket_coverage,
            buckets,
            traffic,
            scoped_traffic: Some(scoped_traffic),
        };
        connections.with_profile(id, |current| {
            ensure_scope(current, &scope)?;
            self.repository.usage(scope.clone(), sample)?;
            self.repository.view(scope, Utc::now())
        })
    }

    pub async fn check(
        &self,
        connections: &ConnectionService,
        id: Uuid,
        mode: CheckMode,
    ) -> Result<MonitoringView> {
        let profile = connections.with_profile(id, |profile| Ok(profile.clone()))?;
        let scope = Scope::from(&profile);
        let signed = (mode == CheckMode::Signed).then(|| {
            let session = connections.session_for(id, Workspace::Manager, IdentityKind::S3)?;
            ensure_scope(&session.profile, &scope)?;
            Ok(session)
        });
        let result = health::probe(&profile, mode, signed).await;
        let now = Utc::now();
        connections.with_profile(id, |current| {
            ensure_scope(current, &scope)?;
            self.repository.health(&profile, mode, result, now)?;
            self.repository.view(scope, now)
        })
    }
}

async fn scoped_traffic(
    session: &Session,
    now: DateTime<Utc>,
) -> Reading<super::model::TrafficPeriod> {
    let account = &session.context.account;
    let tenant = &session.context.tenant;
    if account.is_empty() {
        return Reading::missing(
            Coverage::Unavailable,
            "Set the RGW account or user ID on this profile to read scoped traffic.",
        );
    }
    let account_id = account.strip_prefix("RGW").is_some_and(|suffix| {
        suffix.len() == 17 && suffix.bytes().all(|digit| digit.is_ascii_digit())
    });
    let uid = if account_id || tenant.is_empty() || account.starts_with(&format!("{tenant}$")) {
        account.clone()
    } else {
        format!("{tenant}${account}")
    };
    rgw::traffic(session, now, Some(&uid)).await
}

fn ensure_scope(current: &Connection, expected: &Scope) -> Result<()> {
    if Scope::from(current) != *expected {
        return Err(AppError::Invalid(
            "The connection changed during collection. Try again.".into(),
        ));
    }
    Ok(())
}

struct Ineligible {
    coverage: Coverage,
    message: String,
}

impl Ineligible {
    fn reading<T>(&self) -> Reading<T> {
        Reading::missing(self.coverage, self.message.clone())
    }
}

fn collection_session(
    connections: &ConnectionService,
    profile: &Connection,
    workspace: Workspace,
    identity: IdentityKind,
) -> std::result::Result<Session, Ineligible> {
    let state = profile.capability(identity);
    if identity == IdentityKind::S3 && !profile.settings.manager_enabled {
        return Err(Ineligible {
            coverage: Coverage::Unavailable,
            message: "Manager is disabled for this connection.".into(),
        });
    }
    if !matches!(state, CapabilityState::Detected)
        && !(identity == IdentityKind::S3 && state == CapabilityState::Unknown)
    {
        let coverage = if state == CapabilityState::Denied {
            Coverage::Denied
        } else {
            Coverage::Unavailable
        };
        return Err(Ineligible {
            coverage,
            message: format!(
                "{} identity {}.",
                identity.label(),
                match state {
                    CapabilityState::Denied => "was denied",
                    CapabilityState::Unknown => "requires verification",
                    _ => "is unavailable",
                }
            ),
        });
    }
    connections
        .session_for(profile.id, workspace, identity)
        .and_then(|session| {
            ensure_scope(&session.profile, &Scope::from(profile))?;
            Ok(session)
        })
        .map_err(|error| Ineligible {
            coverage: Coverage::Unavailable,
            message: error.to_string(),
        })
}

async fn collect_buckets(
    profile: &Connection,
    listing: &std::result::Result<Session, Ineligible>,
    quota: &std::result::Result<Session, Ineligible>,
    now: DateTime<Utc>,
) -> (Reading<u64>, Vec<BucketSample>) {
    let (coverage, mut names) = if profile.known_buckets.is_empty() {
        match listing {
            Ok(session) => match BucketService::list(session).await {
                Ok(buckets) => (
                    Coverage::Complete,
                    buckets.into_iter().map(|bucket| bucket.name).collect(),
                ),
                Err(error) => return (reading_error(error), Vec::new()),
            },
            Err(reason) => return (reason.reading(), Vec::new()),
        }
    } else {
        (Coverage::Partial, profile.known_buckets.clone())
    };
    names.sort_unstable();
    names.dedup();
    let mut samples = Vec::with_capacity(names.len());
    for bucket in names {
        let location = Location {
            connection: profile.id,
            bucket: bucket.clone(),
            prefix: String::new(),
        };
        let (usage, (quota, rgw_storage)) = tokio::join!(
            async {
                match listing {
                    Ok(session) => bucket_usage(&session.s3, &location, now).await,
                    Err(reason) => reason.reading(),
                }
            },
            async {
                match quota {
                    Ok(session) => rgw::bucket_info(session, &bucket).await,
                    Err(reason) => (reason.reading(), reason.reading()),
                }
            }
        );

        samples.push(BucketSample {
            bucket,
            collected_at: Utc::now(),
            usage,
            quota,
            rgw_storage,
        });
    }
    let detail = (coverage == Coverage::Partial)
        .then(|| "Coverage is limited to configured bucket names.".into());
    (
        Reading::new(coverage, Some(samples.len() as u64), detail),
        samples,
    )
}
