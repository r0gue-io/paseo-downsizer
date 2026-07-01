//! Shared runtime state between the scheduler loop and the axum API handlers.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;

use crate::config::{Plan, Step};
use crate::model::*;
use crate::packing::compute_packing;
use crate::state::SESSIONS_PER_ERA;
use crate::state_store::Persisted;

/// The whole shared application state.
pub struct Shared {
    pub plan: Arc<Plan>,
    pub inner: RwLock<Inner>,
    pub control_token: Option<String>,
    pub state_path: PathBuf,
}

pub struct Inner {
    pub snapshot: Option<StateSnapshot>,
    pub persisted: Persisted,
    pub dispatcher_armed: bool,
    pub last_error: Option<String>,
    pub health_reasons: Vec<String>,
}

impl Shared {
    pub fn new(
        plan: Arc<Plan>,
        persisted: Persisted,
        control_token: Option<String>,
        state_path: PathBuf,
        dispatcher_armed: bool,
    ) -> Arc<Self> {
        Arc::new(Shared {
            plan,
            inner: RwLock::new(Inner {
                snapshot: None,
                persisted,
                dispatcher_armed,
                last_error: None,
                health_reasons: Vec::new(),
            }),
            control_token,
            state_path,
        })
    }

    /// Persist current progress + history to disk.
    pub fn persist(&self) {
        let inner = self.inner.read();
        if let Err(e) = inner.persisted.save(&self.state_path) {
            tracing::warn!(target: "store", "failed to persist state.json: {e:#}");
        }
    }

    pub fn set_paused(&self, paused: bool) {
        {
            let mut inner = self.inner.write();
            inner.persisted.paused = paused;
        }
        self.persist();
    }

    /// Build the `/api/plan` view from the current snapshot + persisted progress.
    pub fn plan_view(&self) -> PlanView {
        let inner = self.inner.read();
        let plan = &self.plan;
        let snap = inner.snapshot.as_ref();
        let started_at = inner.persisted.started_at.clone();
        let paused = inner.persisted.paused;

        let start_dt: DateTime<Utc> = started_at
            .parse()
            .unwrap_or_else(|_| Utc::now());
        let era_secs = plan.meta.era_hours as i64 * 3600;

        // Active step (first whose targets are not yet met by observed chain state).
        let active_id = snap.and_then(|s| active_step(plan, s)).map(|st| st.id);

        // Era bookkeeping, for computing whether a step's era boundary has arrived
        // (used to distinguish the shutdown milestone's "holding" vs "due" states).
        let abs_era = snap
            .map(|s| s.relay.session_index as u64 / SESSIONS_PER_ERA)
            .unwrap_or(0) as u32;
        let start_era = inner.persisted.start_era.unwrap_or(abs_era);

        // Para ids from the live snapshot, for per-step computed packing.
        let para_ids: Vec<u32> = snap
            .map(|s| s.paras.iter().map(|p| p.id).collect())
            .unwrap_or_default();

        let steps = plan
            .enabled_steps()
            .into_iter()
            .map(|st| {
                let observed = StepObserved {
                    validators: snap.map(|s| s.relay.validators).unwrap_or(0),
                    cores: snap.map(|s| s.relay.cores).unwrap_or(0),
                };
                let done = snap.map(|s| step_met(st, s)).unwrap_or(false);
                let due = abs_era >= start_era.saturating_add(st.era_offset);
                let status = if st.shutdown {
                    // Terminal milestone: "pending" while holding at the floor
                    // through the grace window; "shutdown" once its era arrives.
                    if Some(st.id) == active_id && due {
                        if paused {
                            "paused"
                        } else {
                            "shutdown"
                        }
                    } else {
                        "pending"
                    }
                } else if done {
                    "done"
                } else if Some(st.id) == active_id {
                    if paused {
                        "paused"
                    } else {
                        "active"
                    }
                } else {
                    "pending"
                }
                .to_string();

                let scheduled = start_dt + Duration::seconds(era_secs * st.era_offset as i64);
                let applied_at = inner.persisted.applied_at.get(&st.id).cloned();

                let computed_packing =
                    compute_packing(&para_ids, st.cores, &plan.packing).cores;

                StepView {
                    id: st.id,
                    era_offset: st.era_offset,
                    targets: StepTargets {
                        validators: st.validators,
                        cores: st.cores,
                        min_set_size: st.min_validator_set_size,
                    },
                    status,
                    scheduled_eta: Some(scheduled.to_rfc3339()),
                    applied_at,
                    computed_packing,
                    observed,
                    note: st.note.clone(),
                    shutdown: st.shutdown,
                }
            })
            .collect();

        PlanView {
            started_at,
            era_hours: plan.meta.era_hours,
            current_step_id: active_id,
            mode: plan.dispatch.mode.clone(),
            paused,
            max_finality_lag_blocks: plan.health.max_finality_lag_blocks,
            min_online_fraction: plan.health.min_online_fraction,
            steps,
        }
    }

    pub fn health_view(&self) -> HealthView {
        let inner = self.inner.read();
        let paused = inner.persisted.paused;
        let status = if inner.last_error.is_some() {
            "error"
        } else if paused {
            "paused"
        } else {
            "ok"
        }
        .to_string();
        HealthView {
            status,
            reasons: inner.health_reasons.clone(),
            dispatcher: if inner.dispatcher_armed && !paused {
                "armed"
            } else {
                "idle"
            }
            .to_string(),
            last_error: inner.last_error.clone(),
        }
    }
}

/// A step's targets are met once the observed set/cores are at or below target
/// (the schedule is monotonically decreasing).
pub fn step_met(step: &Step, snap: &StateSnapshot) -> bool {
    // A shutdown step is a terminal milestone, never "met" by target-matching —
    // so it becomes the active step once the numeric floor is reached and stays
    // there (the actual kill is operators stopping nodes, not an on-chain value).
    if step.shutdown {
        return false;
    }
    snap.relay.validators <= step.validators && snap.relay.cores <= step.cores
}

/// The active step: the first enabled step whose targets are not yet met.
pub fn active_step<'a>(plan: &'a Plan, snap: &StateSnapshot) -> Option<&'a Step> {
    plan.enabled_steps()
        .into_iter()
        .find(|st| !step_met(st, snap))
}
