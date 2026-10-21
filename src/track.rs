//! Followed users / packages and their daily snapshots, saved as JSON in the
//! user's data directory (e.g. %APPDATA%\PkgStats\following.json on Windows).

use crate::api::{Kind, Report, Source};
use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// date (YYYY-MM-DD) -> metric label -> value
pub type History = BTreeMap<String, BTreeMap<String, f64>>;

#[derive(Clone, Serialize, Deserialize)]
pub struct Followed {
    pub source: Source,
    pub kind: Kind,
    pub query: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub history: History,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl Followed {
    pub fn key(&self) -> String {
        key(self.source, self.kind, &self.query)
    }

    /// Latest snapshot and its date.
    pub fn latest(&self) -> Option<(&String, &BTreeMap<String, f64>)> {
        self.history.iter().next_back()
    }

    /// Change of `metric` between the latest snapshot and the newest one at
    /// least `days` days older. `None` until enough history exists.
    pub fn change(&self, metric: &str, days: i64) -> Option<f64> {
        let (date, snap) = self.latest()?;
        let now = *snap.get(metric)?;
        let cutoff = parse_date(date)? - Duration::days(days);
        let (_, old) = self
            .history
            .iter()
            .rev()
            .find(|(d, _)| parse_date(d).is_some_and(|d| d <= cutoff))?;
        Some(now - old.get(metric)?)
    }

    /// (date, value) pairs for one metric, oldest first.
    pub fn series(&self, metric: &str) -> Vec<(String, f64)> {
        self.history
            .iter()
            .filter_map(|(d, snap)| Some((d.clone(), *snap.get(metric)?)))
            .collect()
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub my_username: String,
    #[serde(default)]
    pub followed: Vec<Followed>,
}

pub fn key(source: Source, kind: Kind, query: &str) -> String {
    format!("{source:?}/{kind:?}/{}", query.trim().to_lowercase())
}

pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn parse_date(d: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()
}

impl Store {
    pub fn path() -> PathBuf {
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("PkgStats").join("following.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Write then rename, so a crash mid-write can't wipe the history.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    pub fn find(&self, key: &str) -> Option<&Followed> {
        self.followed.iter().find(|f| f.key() == key)
    }

    pub fn is_followed(&self, source: Source, kind: Kind, query: &str) -> bool {
        self.find(&key(source, kind, query)).is_some()
    }

    /// Start following; returns false if it was already followed.
    pub fn follow(&mut self, source: Source, kind: Kind, query: &str) -> bool {
        if self.is_followed(source, kind, query) {
            return false;
        }
        self.followed.push(Followed {
            source,
            kind,
            query: query.trim().to_string(),
            title: query.trim().to_string(),
            url: String::new(),
            history: History::new(),
            last_error: None,
        });
        true
    }

    pub fn unfollow(&mut self, key: &str) {
        self.followed.retain(|f| f.key() != key);
    }

    /// Store today's numbers from a fresh report (replacing any earlier snapshot from today).
    pub fn record(&mut self, key: &str, report: &Report) -> bool {
        let Some(f) = self.followed.iter_mut().find(|f| f.key() == key) else {
            return false;
        };
        f.title = report.title.clone();
        f.url = report.url.clone();
        f.last_error = None;
        let snap = report.tracked();
        if !snap.is_empty() {
            f.history.insert(today(), snap);
        }
        true
    }

    pub fn record_error(&mut self, key: &str, error: String) {
        if let Some(f) = self.followed.iter_mut().find(|f| f.key() == key) {
            f.last_error = Some(error);
        }
    }

    /// Entries without a snapshot for today.
    pub fn stale(&self) -> Vec<Followed> {
        let today = today();
        self.followed.iter().filter(|f| !f.history.contains_key(&today)).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_changes_against_older_snapshots() {
        let mut f = Followed {
            source: Source::GitHub,
            kind: Kind::User,
            query: "x".into(),
            title: String::new(),
            url: String::new(),
            history: History::new(),
            last_error: None,
        };
        for (d, v) in [("2026-10-01", 10.0), ("2026-10-07", 25.0), ("2026-10-08", 30.0)] {
            f.history.insert(d.into(), BTreeMap::from([("Stars".to_string(), v)]));
        }
        assert_eq!(f.change("Stars", 1), Some(5.0));
        assert_eq!(f.change("Stars", 7), Some(20.0));
        assert_eq!(f.change("Stars", 30), None);
        assert_eq!(f.series("Stars").len(), 3);
    }
}
