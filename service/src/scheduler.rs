//! The scheduler loop. On each new relay best block it rebuilds the live state,
//! evaluates health + era progress, and — when an active step's era boundary is
//! reached, soak has elapsed and health is OK — dry-runs and dispatches that
//! step's calls via proxy+sudo. Idempotent and crash-safe: the active step is
//! always derived from observed chain state.

use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;

use crate::chain::ChainClient;
use crate::config::Step;
use crate::dispatch::{next_timeslice, BatchTx, ChainKind, DispatchItem, Dispatcher};
use crate::model::{CorePacking, HistoryEntry};
use crate::packing::compute_packing;
use crate::shared::{active_step, Shared};
use crate::state::{build_state, SESSIONS_PER_ERA};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Continuous auto-dispatch driven by new blocks.
    Auto,
    /// Run a single scheduler tick, then exit.
    Once,
    /// Simulate (dry-run) the next step without submitting, then exit.
    DryRunOnce,
}

pub struct Scheduler {
    pub shared: Arc<Shared>,
    pub relay: ChainClient,
    pub ah: ChainClient,
    pub dispatcher: Option<Dispatcher>,
    pub mode: RunMode,
}

impl Scheduler {
    pub async fn run(mut self) -> Result<()> {
        loop {
            let mut stream = match self.relay.online.stream_best_blocks().await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(target: "sched", "block stream error: {e:#}; reconnecting");
                    let _ = self.relay.reconnect().await;
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    continue;
                }
            };

            while let Some(next) = stream.next().await {
                let best = match next {
                    Ok(block) => block.number(),
                    Err(e) => {
                        tracing::warn!(target: "sched", "block item error: {e:#}");
                        break; // re-subscribe
                    }
                };

                if let Err(e) = self.tick(best).await {
                    tracing::warn!(target: "sched", "tick error: {e:#}");
                    self.set_last_error(Some(format!("{e:#}")));
                }

                if matches!(self.mode, RunMode::Once | RunMode::DryRunOnce) {
                    return Ok(());
                }
            }

            // Stream ended: reconnect and resubscribe.
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    /// One scheduler tick at relay best block `best`.
    async fn tick(&mut self, best: u64) -> Result<()> {
        let plan = self.shared.plan.clone();
        let snapshot = build_state(&self.relay, &self.ah, &plan, best).await?;

        // Health evaluation.
        let lag = snapshot.relay.finality_lag;
        let online = snapshot.relay.online_fraction;
        let mut reasons = Vec::new();
        if lag > plan.health.max_finality_lag_blocks {
            reasons.push(format!(
                "finality lag {lag} > max {}",
                plan.health.max_finality_lag_blocks
            ));
        }
        if online < plan.health.min_online_fraction {
            reasons.push(format!(
                "online fraction {online:.2} < min {:.2}",
                plan.health.min_online_fraction
            ));
        }
        let healthy = reasons.is_empty();

        let abs_era = (snapshot.relay.session_index as u64 / SESSIONS_PER_ERA) as u32;

        // Publish snapshot + init the start-era anchor.
        {
            let mut inner = self.shared.inner.write();
            inner.snapshot = Some(snapshot.clone());
            inner.health_reasons = reasons.clone();
            inner.last_error = None;
            if inner.persisted.start_era.is_none() {
                inner.persisted.start_era = Some(abs_era);
            }
            // Record derived active step id.
            inner.persisted.current_step_id =
                active_step(&plan, &snapshot).map(|s| s.id);
        }
        self.shared.persist();

        let paused = self.shared.inner.read().persisted.paused;

        // --dry-run: simulate the active step and exit, regardless of timing.
        if matches!(self.mode, RunMode::DryRunOnce) {
            if let Some(step) = active_step(&plan, &snapshot).cloned() {
                self.dispatch_step(&snapshot, &step, best, true).await;
            } else {
                tracing::info!(target: "sched", "dry-run: no active step (targets already met)");
            }
            return Ok(());
        }

        // Auto/once dispatch requires: dispatcher present, not paused, auto mode.
        let auto = plan.dispatch.mode == "auto";
        if self.dispatcher.is_none() || paused || !auto {
            // Still allow packing re-assert only when armed/healthy below.
            self.maybe_reassert_packing(&snapshot, best, abs_era, healthy, paused)
                .await;
            return Ok(());
        }

        let active = match active_step(&plan, &snapshot).cloned() {
            Some(s) => s,
            None => {
                // All steps done: keep packing asserted against broker drift.
                self.maybe_reassert_packing(&snapshot, best, abs_era, healthy, paused)
                    .await;
                return Ok(());
            }
        };

        // Terminal shutdown milestone: NEVER dispatched — there is no on-chain
        // "stop the network" call; the kill is validator operators powering off.
        // Once its era arrives, announce once and hold, keeping parachains packed
        // onto the floor cores while the chain is still alive.
        if active.shutdown {
            let start_era = self
                .shared
                .inner
                .read()
                .persisted
                .start_era
                .unwrap_or(abs_era);
            if abs_era >= start_era.saturating_add(active.era_offset) {
                self.announce_shutdown(&active, abs_era);
            }
            self.maybe_reassert_packing(&snapshot, best, abs_era, healthy, paused)
                .await;
            return Ok(());
        }

        // Timing gates: era boundary reached + soak elapsed.
        let (start_era, last_dispatch_era, soak) = {
            let inner = self.shared.inner.read();
            (
                inner.persisted.start_era.unwrap_or(abs_era),
                inner.persisted.last_dispatch_era,
                plan.meta.soak_eras,
            )
        };
        let due = abs_era >= start_era.saturating_add(active.era_offset);
        let soak_ok = last_dispatch_era.is_none_or(|l| abs_era >= l.saturating_add(soak));

        if !due {
            return Ok(());
        }
        if !soak_ok {
            tracing::debug!(target: "sched", "step {} due but soaking", active.id);
            return Ok(());
        }
        if !healthy {
            tracing::warn!(target: "sched", "step {} due but health gate closed: {:?}", active.id, reasons);
            return Ok(());
        }

        // Fire.
        self.dispatch_step(&snapshot, &active, best, false).await;
        Ok(())
    }

    /// Build, dry-run and (unless simulating) submit a step's calls.
    async fn dispatch_step(
        &mut self,
        snapshot: &crate::model::StateSnapshot,
        step: &Step,
        best: u64,
        simulate: bool,
    ) {
        let plan = self.shared.plan.clone();
        let dispatcher = match &self.dispatcher {
            Some(d) => d,
            None => {
                tracing::warn!(target: "sched", "no dispatcher configured (PROXY_SURI/SUDO_ACCOUNT missing)");
                return;
            }
        };

        let para_ids: Vec<u32> = snapshot.paras.iter().map(|p| p.id).collect();
        let packing = compute_packing(&para_ids, step.cores, &plan.packing);
        if !packing.dropped.is_empty() {
            tracing::warn!(target: "sched",
                "step {}: {} paras dropped to on-demand: {:?}",
                step.id, packing.dropped.len(), packing.dropped);
        }

        let begin = next_timeslice(best, dispatcher.resolved.timeslice_period);
        let (relay_items, ah_items) =
            match dispatcher.build_step_items(&plan, step, &packing.cores, begin) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(target: "sched", "step {} build failed: {e:#}", step.id);
                    self.set_last_error(Some(format!("build step {}: {e:#}", step.id)));
                    return;
                }
            };

        let all: Vec<&DispatchItem> = relay_items.iter().chain(ah_items.iter()).collect();

        tracing::info!(target: "sched",
            "step {} {}: {} relay + {} AH calls (begin timeslice {})",
            step.id, if simulate { "SIMULATE" } else { "DISPATCH" },
            relay_items.len(), ah_items.len(), begin);

        // Phase 1: dry-run everything.
        let mut all_ok = true;
        for item in &all {
            let chain = self.chain_for(item.chain);
            match dispatcher.dry_run(chain, item).await {
                Ok(()) => self.record(step.id, item, "dry_run_ok", None, None, None),
                Err(e) => {
                    all_ok = false;
                    self.record(step.id, item, "dry_run_failed", None, None, Some(format!("{e:#}")));
                    tracing::error!(target: "sched", "dry-run failed: {} : {e:#}", item.call_path);
                }
            }
        }

        // Build the atomic per-chain batches that will actually be submitted:
        // one `proxy.proxy(sudo.sudo(utility.batch_all([...])))` per chain, so the
        // whole reconfiguration lands in a single block (no transient repack gaps).
        let relay_refs: Vec<&DispatchItem> = relay_items.iter().collect();
        let ah_refs: Vec<&DispatchItem> = ah_items.iter().collect();
        let mut batches: Vec<BatchTx> = Vec::new();
        if let Some(b) = dispatcher.batch(ChainKind::Relay, &relay_refs) {
            batches.push(b);
        }
        if let Some(b) = dispatcher.batch(ChainKind::AssetHub, &ah_refs) {
            batches.push(b);
        }

        // Phase 2: dry-run each atomic batch as the final gate.
        for b in &batches {
            let chain = self.chain_for(b.chain);
            if let Err(e) = dispatcher.dry_run_batch(chain, b).await {
                all_ok = false;
                tracing::error!(target: "sched", "batch dry-run failed on {}: {e:#}", b.chain.api_name());
            }
        }

        if simulate {
            tracing::info!(target: "sched",
                "step {} SIMULATE complete: {} atomic batch(es), all_ok={}",
                step.id, batches.len(), all_ok);
            return;
        }
        if !all_ok && dispatcher.requires_dry_run() {
            tracing::error!(target: "sched", "step {} aborted: a dry-run failed and dry_run_first is set", step.id);
            self.set_last_error(Some(format!("step {} dry-run failed; aborted", step.id)));
            return;
        }

        // Phase 3: submit each chain's batch atomically.
        for b in &batches {
            let chain = self.chain_for(b.chain);
            match dispatcher.submit_batch(chain, b).await {
                Ok((tx, block)) => {
                    self.record_batch(step.id, b.chain, b.count, "finalized", Some(tx), Some(block), None);
                    tracing::info!(target: "sched", "submitted atomic batch on {} ({} calls)", b.chain.api_name(), b.count);
                }
                Err(e) => {
                    self.record_batch(step.id, b.chain, b.count, "error", None, None, Some(format!("{e:#}")));
                    self.set_last_error(Some(format!("submit batch on {} failed: {e:#}", b.chain.api_name())));
                    tracing::error!(target: "sched", "submit batch failed on {} : {e:#}", b.chain.api_name());
                    return; // next tick re-derives progress from chain state
                }
            }
        }

        // Success: record progress (advance is still validated from chain next tick).
        {
            let mut inner = self.shared.inner.write();
            let abs_era = (snapshot.relay.session_index as u64 / SESSIONS_PER_ERA) as u32;
            inner.persisted.last_dispatch_era = Some(abs_era);
            inner
                .persisted
                .applied_at
                .insert(step.id, Utc::now().to_rfc3339());
        }
        self.shared.persist();
    }

    /// Re-assert packing if the broker overwrote the intended core assignment.
    /// Compares per-core para membership (not shares) and throttles to once per era.
    async fn maybe_reassert_packing(
        &mut self,
        snapshot: &crate::model::StateSnapshot,
        best: u64,
        abs_era: u32,
        healthy: bool,
        paused: bool,
    ) {
        if self.dispatcher.is_none() || paused || !healthy {
            return;
        }
        if self.shared.plan.dispatch.mode != "auto" {
            return;
        }
        let last = self.shared.inner.read().persisted.last_reassert_era;
        if last == Some(abs_era) {
            return;
        }

        let plan = self.shared.plan.clone();
        let para_ids: Vec<u32> = snapshot.paras.iter().map(|p| p.id).collect();
        let want = compute_packing(&para_ids, snapshot.relay.cores, &plan.packing);
        if membership_matches(&want.cores, &snapshot.packing) {
            return;
        }

        tracing::info!(target: "sched", "packing drift detected; re-asserting assign_core");
        let dispatcher = self.dispatcher.as_ref().unwrap();
        let begin = next_timeslice(best, dispatcher.resolved.timeslice_period);
        // Reuse the step build path for the packing calls only by faking a step
        // with the current core/validator targets is unnecessary; issue assign_core
        // directly for each computed core.
        let items: Vec<DispatchItem> = want
            .cores
            .iter()
            .map(|c| dispatcher.assign_core_reassert(begin, c))
            .collect();
        // Re-assert all cores in one atomic batch, like a step.
        let refs: Vec<&DispatchItem> = items.iter().collect();
        if let Some(batch) = dispatcher.batch(ChainKind::Relay, &refs) {
            let chain = self.chain_for(ChainKind::Relay);
            let gated = dispatcher.requires_dry_run()
                && dispatcher.dry_run_batch(chain, &batch).await.is_err();
            if gated {
                tracing::warn!(target: "sched", "packing re-assert dry-run failed; skipping this era");
            } else {
                match dispatcher.submit_batch(chain, &batch).await {
                    Ok((tx, block)) => {
                        self.record_batch(0, ChainKind::Relay, batch.count, "finalized", Some(tx), Some(block), None)
                    }
                    Err(e) => self.record_batch(
                        0,
                        ChainKind::Relay,
                        batch.count,
                        "error",
                        None,
                        None,
                        Some(format!("{e:#}")),
                    ),
                }
            }
        }
        {
            let mut inner = self.shared.inner.write();
            inner.persisted.last_reassert_era = Some(abs_era);
        }
        self.shared.persist();
    }

    fn chain_for(&self, kind: ChainKind) -> &ChainClient {
        match kind {
            ChainKind::Relay => &self.relay,
            ChainKind::AssetHub => &self.ah,
        }
    }

    /// Announce the terminal shutdown milestone exactly once (idempotent via the
    /// persisted `applied_at` map). No on-chain dispatch — the actual kill is
    /// validator operators stopping their nodes.
    fn announce_shutdown(&self, step: &Step, abs_era: u32) {
        if self
            .shared
            .inner
            .read()
            .persisted
            .applied_at
            .contains_key(&step.id)
        {
            return;
        }
        tracing::warn!(target: "sched",
            "★ SHUTDOWN MILESTONE REACHED (era {abs_era}, step {}): grace window over — \
             validator operators must STOP their nodes to halt the network. There is no \
             on-chain stop; see docs/provider-communication.md and the shutdown runbook.",
            step.id);
        let entry = HistoryEntry {
            at: Utc::now().to_rfc3339(),
            step_id: step.id,
            chain: "relay".to_string(),
            call: "network.shutdown".to_string(),
            args_summary: "terminal milestone — operators stop nodes (no on-chain dispatch)"
                .to_string(),
            tx_hash: None,
            status: "shutdown".to_string(),
            block_hash: None,
            error: None,
        };
        {
            let mut inner = self.shared.inner.write();
            inner.persisted.push_history(entry);
            inner
                .persisted
                .applied_at
                .insert(step.id, Utc::now().to_rfc3339());
        }
        self.shared.persist();
    }

    /// Record a submitted atomic batch (one `utility.batch_all` transaction).
    #[allow(clippy::too_many_arguments)]
    fn record_batch(
        &self,
        step_id: u32,
        chain: ChainKind,
        count: usize,
        status: &str,
        tx_hash: Option<String>,
        block_hash: Option<String>,
        error: Option<String>,
    ) {
        let entry = HistoryEntry {
            at: Utc::now().to_rfc3339(),
            step_id,
            chain: chain.api_name().to_string(),
            call: "utility.batch_all".to_string(),
            args_summary: format!("{count} calls (proxy→sudo→batch_all)"),
            tx_hash,
            status: status.to_string(),
            block_hash,
            error,
        };
        {
            let mut inner = self.shared.inner.write();
            inner.persisted.push_history(entry);
        }
        self.shared.persist();
    }

    fn record(
        &self,
        step_id: u32,
        item: &DispatchItem,
        status: &str,
        tx_hash: Option<String>,
        block_hash: Option<String>,
        error: Option<String>,
    ) {
        let entry = HistoryEntry {
            at: Utc::now().to_rfc3339(),
            step_id,
            chain: item.chain.api_name().to_string(),
            call: item.call_path.clone(),
            args_summary: item.args_summary.clone(),
            tx_hash,
            status: status.to_string(),
            block_hash,
            error,
        };
        {
            let mut inner = self.shared.inner.write();
            inner.persisted.push_history(entry);
        }
        self.shared.persist();
    }

    fn set_last_error(&self, err: Option<String>) {
        let mut inner = self.shared.inner.write();
        inner.last_error = err;
    }
}

/// Whether two packings agree on which paras sit on which core (ignoring shares).
fn membership_matches(want: &[CorePacking], have: &[CorePacking]) -> bool {
    use std::collections::{BTreeMap, BTreeSet};
    let index = |ps: &[CorePacking]| -> BTreeMap<u32, BTreeSet<u32>> {
        ps.iter()
            .map(|c| {
                (
                    c.core,
                    c.assignments.iter().map(|a| a.para_id).collect::<BTreeSet<_>>(),
                )
            })
            .collect()
    };
    index(want) == index(have)
}
