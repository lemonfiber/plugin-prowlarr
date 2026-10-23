use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;

use super::Prowlarr;
use crate::instant;
use lemonfiber_ports::service::{Failure, IndexerUse, Indexers, Limits};

const EPOCH: &str = "1970-01-01T00:00:00";

#[derive(Deserialize)]
struct IndexerResource {
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    enable: bool,
    #[serde(default)]
    fields: Vec<SettingField>,
}

#[derive(Deserialize)]
struct SettingField {
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: Option<serde_json::Value>,
}

const QUERY_CAP: &str = "baseSettings.queryLimit";

const GRAB_CAP: &str = "baseSettings.grabLimit";

const CAP_WINDOW: &str = "baseSettings.limitsUnit";

const HOURLY: u64 = 1;

const DAILY_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

const HOURLY_WINDOW: Duration = Duration::from_secs(60 * 60);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexerStatusResource {
    indexer_id: i64,
    #[serde(default)]
    disabled_till: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexerStatsResource {
    #[serde(default)]
    indexers: Vec<IndexerCounts>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexerCounts {
    indexer_id: i64,
    #[serde(default)]
    number_of_queries: i64,
    #[serde(default)]
    number_of_rss_queries: i64,
    #[serde(default)]
    number_of_grabs: i64,
    #[serde(default)]
    number_of_failed_queries: i64,
    #[serde(default)]
    number_of_failed_rss_queries: i64,
    #[serde(default)]
    number_of_failed_grabs: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FirstCalls {
    searched: Option<SystemTime>,
    grabbed: Option<SystemTime>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryRecord {
    indexer_id: i64,
    #[serde(default)]
    date: String,
    #[serde(default)]
    event_type: EventType,
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum EventType {
    Named(String),
    Numbered(i64),
    #[default]
    Unwritten,
}

impl EventType {
    fn is_search(&self) -> bool {
        matches!(self, Self::Named(name) if name == "indexerQuery" || name == "indexerRss")
            || matches!(self, Self::Numbered(2 | 3))
    }

    fn is_grab(&self) -> bool {
        matches!(self, Self::Named(name) if name == "releaseGrabbed")
            || matches!(self, Self::Numbered(1))
    }
}

impl Prowlarr {
    async fn counts_since(&self, start: &str) -> Result<IndexerStatsResource, Failure> {
        self.servarr
            .read(
                &format!("/indexerstats?startDate={start}"),
                "the indexer counts",
            )
            .await
    }

    async fn first_calls(
        &self,
        now: SystemTime,
        listed: &[IndexerResource],
    ) -> Result<BTreeMap<i64, FirstCalls>, Failure> {
        let Some(reach) = longest_capped_window(listed) else {
            return Ok(BTreeMap::new());
        };
        let history: Vec<HistoryRecord> = self
            .servarr
            .read(
                &format!("/history/since?date={}", window_start(now, reach)),
                "the indexer history",
            )
            .await?;
        let windows = cutoffs(now, listed);
        let mut first: BTreeMap<i64, FirstCalls> = BTreeMap::new();
        for record in history {
            let (Some(at), Some(cutoff)) =
                (instant::read(&record.date), windows.get(&record.indexer_id))
            else {
                continue;
            };
            if at < *cutoff {
                continue;
            }
            let calls = first.entry(record.indexer_id).or_default();
            if record.event_type.is_search() {
                calls.searched = Some(calls.searched.map_or(at, |first| first.min(at)));
            } else if record.event_type.is_grab() {
                calls.grabbed = Some(calls.grabbed.map_or(at, |first| first.min(at)));
            }
        }
        Ok(first)
    }
}

#[async_trait]
impl Indexers for Prowlarr {
    async fn indexers(&self, now: SystemTime) -> Result<Vec<IndexerUse>, Failure> {
        indexers(self, now).await
    }
}

fn window_start(now: SystemTime, window: Duration) -> String {
    let since = now.checked_sub(window).unwrap_or(UNIX_EPOCH);
    instant::written(since).unwrap_or_else(|| EPOCH.to_owned())
}

fn windows_in_use(listed: &[IndexerResource]) -> BTreeSet<Duration> {
    listed
        .iter()
        .filter(|indexer| indexer.enable)
        .map(window_of)
        .collect()
}

fn cutoffs(now: SystemTime, listed: &[IndexerResource]) -> BTreeMap<i64, SystemTime> {
    listed
        .iter()
        .filter(|indexer| indexer.enable && limits_of(indexer).is_some())
        .map(|indexer| {
            (
                indexer.id,
                now.checked_sub(window_of(indexer)).unwrap_or(UNIX_EPOCH),
            )
        })
        .collect()
}

fn longest_capped_window(listed: &[IndexerResource]) -> Option<Duration> {
    listed
        .iter()
        .filter(|indexer| indexer.enable && limits_of(indexer).is_some())
        .map(window_of)
        .max()
}

fn window_of(indexer: &IndexerResource) -> Duration {
    if setting(indexer, CAP_WINDOW) == Some(HOURLY) {
        HOURLY_WINDOW
    } else {
        DAILY_WINDOW
    }
}

fn limits_of(indexer: &IndexerResource) -> Option<Limits> {
    let queries = setting(indexer, QUERY_CAP);
    let grabs = setting(indexer, GRAB_CAP);
    (queries.is_some() || grabs.is_some()).then(|| Limits {
        queries,
        grabs,
        window: window_of(indexer),
    })
}

fn setting(indexer: &IndexerResource, name: &str) -> Option<u64> {
    indexer
        .fields
        .iter()
        .find(|field| field.name == name)?
        .value
        .as_ref()?
        .as_u64()
}

fn indexer_use(
    indexer: IndexerResource,
    counts: Option<&IndexerCounts>,
    rested_until: Option<String>,
    limits: Option<Limits>,
    calls: FirstCalls,
) -> IndexerUse {
    let count = |taken: fn(&IndexerCounts) -> i64| {
        counts.map_or(0, |counts| u64::try_from(taken(counts)).unwrap_or(0))
    };
    IndexerUse {
        name: indexer.name,
        enabled: indexer.enable,
        queries: count(|counts| counts.number_of_queries)
            .saturating_add(count(|counts| counts.number_of_rss_queries)),
        failed_queries: count(|counts| counts.number_of_failed_queries)
            .saturating_add(count(|counts| counts.number_of_failed_rss_queries)),
        grabs: count(|counts| counts.number_of_grabs),
        failed_grabs: count(|counts| counts.number_of_failed_grabs),
        rested_until,
        limits,
        searched_from: calls.searched,
        grabbed_from: calls.grabbed,
    }
}

async fn indexers(prowlarr: &Prowlarr, now: SystemTime) -> Result<Vec<IndexerUse>, Failure> {
    let listed: Vec<IndexerResource> = prowlarr.servarr.read("/indexer", "the indexers").await?;
    let standing: Vec<IndexerStatusResource> = prowlarr
        .servarr
        .read("/indexerstatus", "the indexer standings")
        .await?;
    let mut counted: BTreeMap<u64, IndexerStatsResource> = BTreeMap::new();
    for window in windows_in_use(&listed) {
        let stats = prowlarr.counts_since(&window_start(now, window)).await?;
        counted.insert(window.as_secs(), stats);
    }
    let first = prowlarr.first_calls(now, &listed).await?;
    Ok(listed
        .into_iter()
        .map(|indexer| {
            let limits = limits_of(&indexer);
            let counts = counted
                .get(&window_of(&indexer).as_secs())
                .and_then(|stats| {
                    stats
                        .indexers
                        .iter()
                        .find(|counts| counts.indexer_id == indexer.id)
                });
            let rested = standing
                .iter()
                .find(|status| status.indexer_id == indexer.id)
                .and_then(|status| status.disabled_till.clone());
            let calls = first.get(&indexer.id).copied().unwrap_or_default();
            indexer_use(indexer, counts, rested, limits, calls)
        })
        .collect())
}

#[cfg(test)]
mod tests;
