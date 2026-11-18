use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::{
        document::{read_document, write_document},
        error::{AppError, Result},
    },
    connection::model::Connection,
};

use super::{
    health::{ProbeOutcome, apply_incident},
    history::{self, DAILY_SLOTS, DAY_SECONDS, HOUR_SECONDS, HOURLY_SLOTS},
    model::{CheckMode, HealthSample, Incident, MonitoringView, Scope, UsagePoint, UsageSample},
};

const RETENTION: TimeDelta = TimeDelta::days(90);
const HEALTH_LIMIT: usize = 1000;

#[derive(Clone, Default, Deserialize, Serialize)]
struct Timeline {
    usage: Option<UsageSample>,
    hourly: Vec<UsagePoint>,
    daily: Vec<UsagePoint>,
    health: Vec<HealthSample>,
    incidents: Vec<Incident>,
}

#[derive(Clone, Deserialize, Serialize)]
struct ScopedTimeline {
    scope: Scope,
    timeline: Timeline,
}

pub struct MonitoringRepository {
    directory: PathBuf,
    timelines: Mutex<BTreeMap<Uuid, ScopedTimeline>>,
}

impl MonitoringRepository {
    pub fn open(directory: PathBuf) -> Result<Self> {
        let timelines = read_document(&directory.join("monitoring.json"))?;
        Ok(Self {
            directory,
            timelines: Mutex::new(timelines),
        })
    }

    pub fn view(&self, scope: Scope, now: DateTime<Utc>) -> Result<MonitoringView> {
        let timelines = self.timelines.lock().map_err(|_| unavailable())?;
        let empty = Timeline::default();
        let timeline = timelines
            .get(&scope.connection_id)
            .filter(|entry| entry.scope == scope)
            .map_or(&empty, |entry| &entry.timeline);
        let usage = timeline
            .usage
            .as_ref()
            .filter(|sample| now - sample.collected_at <= RETENTION);
        Ok(MonitoringView {
            scope,
            retrieved_at: now,
            stale: usage.is_some_and(|sample| now - sample.collected_at >= TimeDelta::hours(1)),
            usage: usage.cloned(),
            hourly: history::slots(&timeline.hourly, now, HOUR_SECONDS, HOURLY_SLOTS),
            daily: history::slots(&timeline.daily, now, DAY_SECONDS, DAILY_SLOTS),
            health: timeline
                .health
                .iter()
                .rev()
                .filter(|sample| now - sample.checked_at <= RETENTION)
                .cloned()
                .collect(),
            incidents: timeline
                .incidents
                .iter()
                .rev()
                .filter(|incident| incident.recovered_at.is_none_or(|at| now - at <= RETENTION))
                .cloned()
                .collect(),
        })
    }

    pub fn usage(&self, scope: Scope, sample: UsageSample) -> Result<()> {
        self.update(scope, |timeline| {
            if timeline
                .usage
                .as_ref()
                .is_some_and(|previous| previous.collected_at > sample.collected_at)
            {
                return;
            }
            let point = history::point(&sample);
            history::insert(
                &mut timeline.hourly,
                point.clone(),
                HOUR_SECONDS,
                HOURLY_SLOTS,
            );
            history::insert(&mut timeline.daily, point, DAY_SECONDS, DAILY_SLOTS);
            timeline.usage = Some(sample);
        })
    }

    pub fn health(
        &self,
        profile: &Connection,
        mode: CheckMode,
        outcome: ProbeOutcome,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.update(profile.into(), |timeline| {
            if timeline
                .health
                .last()
                .is_some_and(|previous| previous.checked_at > now)
            {
                return;
            }
            timeline
                .health
                .retain(|sample| now - sample.checked_at <= RETENTION);
            timeline
                .incidents
                .retain(|incident| incident.recovered_at.is_none_or(|at| now - at <= RETENTION));
            let sample = outcome.sample(profile, mode, now, &timeline.health);
            apply_incident(&mut timeline.incidents, &sample);
            timeline.health.push(sample);
            let overflow = timeline.health.len().saturating_sub(HEALTH_LIMIT);
            timeline.health.drain(..overflow);
        })
    }

    fn update(&self, scope: Scope, apply: impl FnOnce(&mut Timeline)) -> Result<()> {
        let mut timelines = self.timelines.lock().map_err(|_| unavailable())?;
        let mut updated = timelines.clone();
        let entry = updated
            .entry(scope.connection_id)
            .or_insert_with(|| ScopedTimeline {
                scope: scope.clone(),
                timeline: Timeline::default(),
            });
        if entry.scope != scope {
            *entry = ScopedTimeline {
                scope,
                timeline: Timeline::default(),
            };
        }
        apply(&mut entry.timeline);
        write_document(
            &self.directory,
            &self.directory.join("monitoring.json"),
            &updated,
        )?;
        *timelines = updated;
        Ok(())
    }
}

fn unavailable() -> AppError {
    AppError::File("Monitoring history is unavailable.".into())
}
