use chrono::{DateTime, TimeDelta, Utc};

use super::model::{Coverage, HistorySlot, UsagePoint, UsageSample};

pub const HOURLY_SLOTS: usize = 48;
pub const DAILY_SLOTS: usize = 90;
pub const HOUR_SECONDS: i64 = 3600;
pub const DAY_SECONDS: i64 = 24 * HOUR_SECONDS;

pub fn point(sample: &UsageSample) -> UsagePoint {
    let measured = sample
        .buckets
        .iter()
        .filter_map(|bucket| bucket.usage.value.as_ref());
    let current_bytes = sum_measured(measured.clone().map(|usage| usage.current_bytes), sample);
    let current_objects = sum_measured(measured.map(|usage| usage.current_objects), sample);
    let coverage = if sample.bucket_coverage.value.is_none() {
        sample.bucket_coverage.coverage
    } else if sample.bucket_coverage.coverage == Coverage::Partial
        || sample.buckets.iter().any(|bucket| {
            !matches!(
                bucket.usage.coverage,
                Coverage::Complete | Coverage::CurrentOnly
            )
        })
    {
        Coverage::Partial
    } else if sample
        .buckets
        .iter()
        .any(|bucket| bucket.usage.coverage == Coverage::CurrentOnly)
    {
        Coverage::CurrentOnly
    } else {
        Coverage::Complete
    };
    UsagePoint {
        collected_at: sample.collected_at,
        coverage,
        current_bytes,
        current_objects,
    }
}

fn sum_measured(values: impl Iterator<Item = Option<u64>>, sample: &UsageSample) -> Option<u64> {
    if sample.bucket_coverage.value == Some(0) {
        return Some(0);
    }
    values.flatten().reduce(u64::saturating_add)
}

pub fn slots(
    samples: &[UsagePoint],
    now: DateTime<Utc>,
    seconds: i64,
    count: usize,
) -> Vec<HistorySlot> {
    let end = period_start(now, seconds);
    let mut samples = samples.iter().rev().peekable();
    (0..count)
        .map(|offset| {
            let start = end - TimeDelta::seconds(seconds * offset as i64);
            let until = start + TimeDelta::seconds(seconds);
            while samples
                .peek()
                .is_some_and(|sample| sample.collected_at >= until)
            {
                samples.next();
            }
            let sample = samples
                .peek()
                .filter(|sample| sample.collected_at >= start)
                .map(|sample| (*sample).clone());
            HistorySlot {
                period_start: start,
                sample,
            }
        })
        .collect()
}

pub fn insert(points: &mut Vec<UsagePoint>, point: UsagePoint, seconds: i64, count: usize) {
    let start = period_start(point.collected_at, seconds);
    let oldest = start - TimeDelta::seconds(seconds * (count as i64 - 1));
    points.retain(|sample| sample.collected_at >= oldest && sample.collected_at < start);
    points.push(point);
}

fn period_start(now: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(now.timestamp().div_euclid(seconds) * seconds, 0).unwrap_or(now)
}
