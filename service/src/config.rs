//! Configuration: the authored plan (`plan/downsizing-plan.toml`) plus runtime
//! settings from the environment. The plan is INTENT (targets + weights); the
//! chain is STATE. Nothing on-chain is hardcoded here.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

/// The full parsed plan file.
#[derive(Debug, Clone, Deserialize)]
pub struct Plan {
    pub meta: Meta,
    pub dispatch: Dispatch,
    pub health: Health,
    #[serde(default, rename = "step")]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub exit_cohort: BTreeMap<String, Vec<String>>,
    pub packing: Packing,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Meta {
    pub name: String,
    pub relay_ws: String,
    pub asset_hub_ws: String,
    pub era_hours: u32,
    #[serde(default = "default_soak")]
    pub soak_eras: u32,
    /// Optional go-live timestamp (RFC3339, e.g. "2026-07-02T12:00:00+02:00").
    /// Until this instant the service stays ARMED but does NOT touch the chain —
    /// it just serves the dashboard. At/after it, the schedule anchors to the
    /// current era and begins. Absolute, so it's restart-safe.
    #[serde(default)]
    pub start_at: Option<String>,
    /// Pre-downsize validator-set size. Informational only: used to render the
    /// `from` count in the cycle-1 `--matrix-test` preview. The real per-cycle
    /// notices always read the live count from chain, never this.
    #[serde(default)]
    pub start_validators: Option<u32>,
}

impl Meta {
    /// Parsed go-live time, if configured and valid.
    pub fn start_at_dt(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.start_at.as_deref().and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        })
    }
}

fn default_soak() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dispatch {
    /// "auto" fires automatically at the scheduled era; anything else is manual.
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_true")]
    pub dry_run_first: bool,
    /// Deserialized from the plan; health is always enforced by the scheduler.
    #[serde(default = "default_true")]
    #[allow(dead_code)]
    pub require_healthy_finality: bool,
}

fn default_mode() -> String {
    "auto".to_string()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct Health {
    #[serde(default = "default_max_lag")]
    pub max_finality_lag_blocks: u64,
    #[serde(default = "default_min_online")]
    pub min_online_fraction: f64,
}

fn default_max_lag() -> u64 {
    6
}
fn default_min_online() -> f64 {
    0.80
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub id: u32,
    pub era_offset: u32,
    pub validators: u32,
    pub cores: u32,
    pub min_validator_set_size: u32,
    #[serde(default)]
    pub note: String,
    /// Terminal milestone: the coordinated network shutdown. The service does
    /// NOT dispatch a reduction for it (there is no on-chain "stop the network");
    /// it marks the milestone and the operators stop nodes. `validators`/`cores`
    /// on a shutdown step are informational (the floor).
    #[serde(default)]
    pub shutdown: bool,
}

impl Step {
    /// The exit-cohort key in the plan (`step_<id>`).
    pub fn cohort_key(&self) -> String {
        format!("step_{}", self.id)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Packing {
    pub max_chains_per_core: u32,
    pub hard_max_chains_per_core: u32,
    pub default_weight: u32,
    #[serde(default)]
    pub weights: BTreeMap<String, u32>,
}

impl Packing {
    /// Weight for a para id: explicit weight or `default_weight`.
    pub fn weight_for(&self, para_id: u32) -> u32 {
        self.weights
            .get(&para_id.to_string())
            .copied()
            .unwrap_or(self.default_weight)
    }
}

impl Plan {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading plan file {}", path.display()))?;
        let plan: Plan =
            toml::from_str(&raw).with_context(|| format!("parsing plan file {}", path.display()))?;
        Ok(plan)
    }

    /// All plan steps, in order (including the terminal shutdown step).
    pub fn enabled_steps(&self) -> Vec<&Step> {
        self.steps.iter().collect()
    }

    /// `(cycle_id, from, to)` validator counts for each non-shutdown step, for
    /// rendering preview/smoke-test notices without a chain connection. `from` of
    /// the first cycle uses `meta.start_validators` (falling back to that step's
    /// own target if unset); each later cycle's `from` is the prior step's target.
    pub fn preview_cycles(&self) -> Vec<(u32, u32, u32)> {
        let mut out = Vec::new();
        let mut prev = self.meta.start_validators;
        for s in self.steps.iter().filter(|s| !s.shutdown) {
            let from = prev.unwrap_or(s.validators);
            out.push((s.id, from, s.validators));
            prev = Some(s.validators);
        }
        out
    }

    /// The exit-cohort stashes (ss58 strings) for a given step id.
    pub fn exit_cohort_for(&self, step: &Step) -> Vec<String> {
        self.exit_cohort
            .get(&step.cohort_key())
            .cloned()
            .unwrap_or_default()
    }
}

/// Runtime settings from environment (+ CLI overrides for the plan path).
#[derive(Debug, Clone)]
pub struct Settings {
    /// SURI of the proxy delegate key. Never persisted.
    pub proxy_suri: Option<String>,
    /// ss58 of the real sudo account the proxy acts for.
    pub sudo_account: Option<String>,
    pub relay_ws: String,
    pub ah_ws: String,
    pub control_token: Option<String>,
    pub bind_addr: String,
    /// Matrix notifier (per-cycle offboarding posts). All three required to enable.
    pub matrix_homeserver: Option<String>,
    pub matrix_token: Option<String>,
    pub matrix_room: Option<String>,
}

impl Settings {
    /// Build settings from env, using the plan for endpoint defaults.
    pub fn from_env(plan: &Plan) -> Self {
        let relay_ws = std::env::var("RELAY_WS").unwrap_or_else(|_| plan.meta.relay_ws.clone());
        let ah_ws = std::env::var("AH_WS").unwrap_or_else(|_| plan.meta.asset_hub_ws.clone());
        Settings {
            proxy_suri: env_opt("PROXY_SURI"),
            sudo_account: env_opt("SUDO_ACCOUNT"),
            relay_ws,
            ah_ws,
            control_token: env_opt("CONTROL_TOKEN"),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            matrix_homeserver: env_opt("MATRIX_HOMESERVER"),
            matrix_token: env_opt("MATRIX_TOKEN"),
            matrix_room: env_opt("MATRIX_ROOM"),
        }
    }
}

fn env_opt(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}
