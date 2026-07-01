//! Crash-safe persistence of dispatch history + scheduler progress to a local
//! `state.json` (next to the binary / cwd). Keys are never written here.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::model::HistoryEntry;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Persisted {
    pub started_at: String,
    /// Absolute era index observed at first startup (anchor for era offsets).
    pub start_era: Option<u32>,
    /// Absolute era index of the last successful dispatch (soak anchor).
    pub last_dispatch_era: Option<u32>,
    /// Absolute era of the last packing re-assertion (throttle).
    pub last_reassert_era: Option<u32>,
    /// Recorded current step id (authoritative truth is derived from chain).
    pub current_step_id: Option<u32>,
    pub paused: bool,
    /// step id -> ISO timestamp when its targets were dispatched.
    pub applied_at: BTreeMap<u32, String>,
    pub history: Vec<HistoryEntry>,
}

impl Default for Persisted {
    fn default() -> Self {
        Persisted {
            started_at: Utc::now().to_rfc3339(),
            start_era: None,
            last_dispatch_era: None,
            last_reassert_era: None,
            current_step_id: None,
            paused: false,
            applied_at: BTreeMap::new(),
            history: Vec::new(),
        }
    }
}

impl Persisted {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                tracing::warn!(target: "store", "state.json unreadable ({e}); starting fresh");
                Persisted::default()
            }),
            Err(_) => Persisted::default(),
        }
    }

    /// Atomically write to `path` (write temp + rename).
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp: PathBuf = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self).context("serializing state.json")?;
        std::fs::write(&tmp, json).context("writing state.json.tmp")?;
        std::fs::rename(&tmp, path).context("renaming state.json into place")?;
        Ok(())
    }

    pub fn push_history(&mut self, entry: HistoryEntry) {
        self.history.push(entry);
        // Bound the log so state.json cannot grow without limit.
        const MAX: usize = 2000;
        if self.history.len() > MAX {
            let excess = self.history.len() - MAX;
            self.history.drain(0..excess);
        }
    }
}
