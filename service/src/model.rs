//! Serde types for the Service <-> UI JSON API contract (see SPEC "Service ↔ UI
//! API contract"). All timestamps are ISO-8601 UTC.

use serde::{Deserialize, Serialize};

pub type Iso8601 = String;

/// `GET /api/state` — live chain snapshot.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    pub fetched_at: Iso8601,
    pub relay: RelayState,
    pub asset_hub: AssetHubState,
    pub paras: Vec<ParaInfo>,
    /// CURRENT on-chain packing (derived from claim_queue).
    pub packing: Vec<CorePacking>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayState {
    pub chain: String,
    pub spec_version: u32,
    pub best_block: u64,
    pub finalized_block: u64,
    pub finality_lag: u64,
    pub session_index: u32,
    pub era_progress: EraProgress,
    pub validators: u32,
    pub min_validator_set_size: u32,
    pub cores: u32,
    pub ah_client_mode: String,
    pub validator_groups: ValidatorGroups,
    /// Best-effort online fraction of the active set (>= this must hold to dispatch).
    pub online_fraction: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EraProgress {
    pub current_era: u32,
    pub session_in_era: u32,
    pub blocks_into_era: u64,
    pub era_length_blocks: u64,
    pub next_era_eta: Iso8601,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorGroups {
    pub count: u32,
    pub sizes: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetHubState {
    pub chain: String,
    pub best_block: u64,
    pub finalized_block: u64,
    pub validator_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParaInfo {
    pub id: u32,
    pub name: String,
    pub cores: u32,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorePacking {
    pub core: u32,
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub para_id: u32,
    pub parts: u32,
    pub share_fraction: f64,
    pub expected_block_time_secs: f64,
}

/// `GET /api/plan` — the schedule with per-step status derived from chain state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub started_at: Iso8601,
    /// Configured go-live time (RFC3339); null if starting immediately. Before
    /// it, the service is armed but holding (dashboard shows a countdown).
    pub starts_at: Option<Iso8601>,
    pub era_hours: u32,
    pub current_step_id: Option<u32>,
    pub mode: String,
    pub paused: bool,
    /// Health gates (from plan `[health]`) so the UI renders the same thresholds
    /// the scheduler enforces, without hardcoding them.
    pub max_finality_lag_blocks: u64,
    pub min_online_fraction: f64,
    pub steps: Vec<StepView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    pub id: u32,
    pub era_offset: u32,
    pub targets: StepTargets,
    pub status: String, // done|active|pending|paused|failed|shutdown
    pub scheduled_eta: Option<Iso8601>,
    pub applied_at: Option<Iso8601>,
    pub computed_packing: Vec<CorePacking>,
    pub observed: StepObserved,
    pub note: String,
    /// True for the terminal shutdown milestone (rendered distinctly).
    pub shutdown: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepTargets {
    pub validators: u32,
    pub cores: u32,
    pub min_set_size: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepObserved {
    pub validators: u32,
    pub cores: u32,
}

/// `GET /api/history` — dispatched txs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub at: Iso8601,
    pub step_id: u32,
    pub chain: String, // relay|assetHub
    pub call: String,
    pub args_summary: String,
    pub tx_hash: Option<String>,
    pub status: String, // in_block|finalized|dry_run_failed|error|dry_run_ok
    pub block_hash: Option<String>,
    pub error: Option<String>,
}

/// `GET /api/health`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthView {
    pub status: String, // ok|paused|error
    pub reasons: Vec<String>,
    pub dispatcher: String, // armed|idle
    pub last_error: Option<String>,
}

/// `POST /api/control` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlRequest {
    pub action: String, // pause|resume
}
