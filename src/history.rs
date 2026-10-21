//! Search history, saved as JSON next to the followed list
//! (e.g. %APPDATA%\PkgStats\history.json on Windows).

use crate::api::Kind;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_ENTRIES: usize = 200;

#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub kind: Kind,
    pub query: String,
    /// Enabled sources, in `Source::ALL` order.
    pub sources: [bool; 4],
    /// Local time, "YYYY-MM-DD HH:MM".
    pub at: String,
}

impl Entry {
    pub fn date(&self) -> &str {
        self.at.get(..10).unwrap_or(&self.at)
    }

    pub fn time(&self) -> &str {
        self.at.get(11..).unwrap_or("")
    }
}

/// Newest first.
#[derive(Default, Serialize, Deserialize)]
pub struct History {
    #[serde(default)]
    pub entries: Vec<Entry>,
}

impl History {
    pub fn path() -> PathBuf {
        crate::track::Store::path().with_file_name("history.json")
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
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    /// Record a search; repeating one moves it to the top instead of duplicating it.
    pub fn add(&mut self, kind: Kind, query: &str, sources: [bool; 4]) {
        let query = query.trim().to_string();
        self.entries.retain(|e| !(e.kind == kind && e.query == query && e.sources == sources));
        let at = Local::now().format("%Y-%m-%d %H:%M").to_string();
        self.entries.insert(0, Entry { kind, query, sources, at });
        self.entries.truncate(MAX_ENTRIES);
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_search_moves_to_top() {
        let mut h = History::default();
        h.add(Kind::User, "a", [true; 4]);
        h.add(Kind::User, "b", [true; 4]);
        h.add(Kind::User, " a ", [true; 4]);
        let order: Vec<&str> = h.entries.iter().map(|e| e.query.as_str()).collect();
        assert_eq!(order, ["a", "b"]);
        h.add(Kind::Package, "a", [true; 4]);
        assert_eq!(h.entries.len(), 3);
    }
}
