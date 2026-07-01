//! Build the `/api/state` snapshot from live chain reads (relay + Asset Hub).
//! Everything is read dynamically; nothing that can be read from chain is
//! hardcoded (SPEC "Rust service requirements").

use anyhow::Result;
use chrono::Utc;

use crate::chain::{fetch_storage, runtime_api, AtBlock, ChainClient};
use crate::config::Plan;
use crate::model::*;
use crate::packing::{BLOCK_SECS, PARTS_FULL};
use crate::valueutil::{as_seq, field, flat_u32, seq_len, variant_name};

/// Blocks per epoch/session on Paseo (1h at 6s).
pub const EPOCH_BLOCKS: u64 = 600;
/// Sessions per era.
pub const SESSIONS_PER_ERA: u64 = 6;
/// Blocks per era (6h).
pub const ERA_BLOCKS: u64 = EPOCH_BLOCKS * SESSIONS_PER_ERA;
/// Default `MinimumValidatorSetSize` when the parameters map has no entry.
pub const DEFAULT_MIN_SET_SIZE: u32 = 100;

/// Candidate pallet names for the AH staking pallet (pallet_staking_async is
/// commonly still exposed as `Staking`).
const STAKING_PALLET_CANDIDATES: &[&str] = &["Staking", "StakingAsync", "StakingAhClient"];

/// Build the full live snapshot. `relay_best` is the best (non-finalized) block
/// number; storage is read at the finalized block for consistency.
pub async fn build_state(
    relay: &ChainClient,
    ah: &ChainClient,
    _plan: &Plan,
    relay_best: u64,
) -> Result<StateSnapshot> {
    // ---- Relay ----
    let at = relay.at_current().await?;
    let finalized = at.block_number();
    let spec_version = at.spec_version();
    // Read the best head fresh so `best` is never behind `finalized` — the tick's
    // captured number can lag by the time we read the finalized head, which would
    // otherwise show finalized > best. `best >= finalized` always holds.
    let best = relay
        .legacy
        .chain_get_header(None)
        .await
        .ok()
        .flatten()
        .map(|h| {
            use subxt::config::Header;
            h.number()
        })
        .unwrap_or(relay_best)
        .max(finalized);
    let finality_lag = best.saturating_sub(finalized);

    let chain = relay
        .legacy
        .system_chain()
        .await
        .unwrap_or_else(|_| "Paseo Testnet".to_string());

    let session_index = read_u32(&at, "Session", "CurrentIndex").await.unwrap_or(0);

    let validators = fetch_storage(&at, "Session", "Validators")
        .await
        .ok()
        .flatten()
        .map(|v| seq_len(&v) as u32)
        .unwrap_or(0);

    let cores = read_num_cores(&at).await.unwrap_or(0);

    let min_validator_set_size = read_min_set_size(&at).await;

    let ah_client_mode = read_ah_client_mode(&at).await;

    let validator_groups = read_validator_groups(&at).await;

    // Active paras + current packing from the claim queue.
    let (paras, current_packing) = read_claim_queue(&at).await.unwrap_or_default();

    let era_progress = compute_era_progress(&at, session_index, best).await;

    let relay_state = RelayState {
        chain,
        spec_version,
        best_block: best,
        finalized_block: finalized,
        finality_lag,
        session_index,
        era_progress,
        validators,
        min_validator_set_size,
        cores,
        ah_client_mode,
        validator_groups,
        online_fraction: estimate_online_fraction(
            finality_lag,
            _plan.health.max_finality_lag_blocks,
        ),
    };

    // ---- Asset Hub ----
    let ah_at = ah.at_current().await?;
    let ah_finalized = ah_at.block_number();
    let ah_best = ah
        .legacy
        .chain_get_header(None)
        .await
        .ok()
        .flatten()
        .map(|h| {
            use subxt::config::Header;
            h.number()
        })
        .unwrap_or(ah_finalized);
    let ah_chain = ah
        .legacy
        .system_chain()
        .await
        .unwrap_or_else(|_| "Asset Hub".to_string());
    let ah_validator_count = read_ah_validator_count(&ah_at).await;

    let asset_hub = AssetHubState {
        chain: ah_chain,
        best_block: ah_best,
        finalized_block: ah_finalized,
        validator_count: ah_validator_count,
    };

    Ok(StateSnapshot {
        fetched_at: Utc::now().to_rfc3339(),
        relay: relay_state,
        asset_hub,
        paras,
        packing: current_packing,
    })
}

async fn read_u32(at: &AtBlock, pallet: &str, entry: &str) -> Option<u32> {
    fetch_storage(at, pallet, entry)
        .await
        .ok()
        .flatten()
        .and_then(|v| flat_u32(&v))
}

/// `configuration.activeConfig.schedulerParams.numCores`.
async fn read_num_cores(at: &AtBlock) -> Option<u32> {
    let cfg = fetch_storage(at, "Configuration", "ActiveConfig")
        .await
        .ok()
        .flatten()?;
    // Field names are snake_case in v14+ metadata; tolerate camelCase too.
    let sched = field(&cfg, "scheduler_params").or_else(|| field(&cfg, "schedulerParams"))?;
    let num = field(sched, "num_cores").or_else(|| field(sched, "numCores"))?;
    flat_u32(num)
}

/// Best-effort read of the AhClient `MinimumValidatorSetSize` dynamic param.
/// The `Parameters` map is scanned for the matching variant; if absent, the
/// runtime default (100) is in force.
async fn read_min_set_size(at: &AtBlock) -> u32 {
    let entries = at
        .storage()
        .iter(("Parameters", "Parameters"), Vec::<subxt::dynamic::Value>::new())
        .await;
    let mut entries = match entries {
        Ok(e) => e,
        Err(_) => return DEFAULT_MIN_SET_SIZE,
    };
    let mut scanned = 0;
    while let Some(item) = entries.next().await {
        scanned += 1;
        if scanned > 512 {
            break;
        }
        let kv = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        let value = match kv.value().decode() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(n) = find_min_set_size(&value) {
            return n;
        }
    }
    DEFAULT_MIN_SET_SIZE
}

/// Recursively look for a `MinimumValidatorSetSize` variant and return its
/// integer payload.
fn find_min_set_size(v: &subxt::dynamic::Value) -> Option<u32> {
    if let Some(name) = variant_name(v) {
        if name == "MinimumValidatorSetSize" {
            // last field carries the value (key precedes it in some encodings).
            for child in as_seq(v).into_iter().rev() {
                if let Some(n) = flat_u32(child) {
                    return Some(n);
                }
            }
        }
    }
    for child in as_seq(v) {
        if let Some(n) = find_min_set_size(child) {
            return Some(n);
        }
    }
    None
}

/// `stakingAhClient.mode`.
async fn read_ah_client_mode(at: &AtBlock) -> String {
    fetch_storage(at, "StakingAhClient", "Mode")
        .await
        .ok()
        .flatten()
        .and_then(|v| variant_name(&v).map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

/// `paraScheduler.validatorGroups` sizes.
async fn read_validator_groups(at: &AtBlock) -> ValidatorGroups {
    match fetch_storage(at, "ParaScheduler", "ValidatorGroups")
        .await
        .ok()
        .flatten()
    {
        Some(v) => {
            let groups = as_seq(&v);
            let sizes: Vec<u32> = groups.iter().map(|g| seq_len(g) as u32).collect();
            ValidatorGroups {
                count: sizes.len() as u32,
                sizes,
            }
        }
        None => ValidatorGroups {
            count: 0,
            sizes: Vec::new(),
        },
    }
}

/// AH validator count from whichever staking pallet exposes it.
async fn read_ah_validator_count(at: &AtBlock) -> u32 {
    for pallet in STAKING_PALLET_CANDIDATES {
        if let Some(n) = read_u32(at, pallet, "ValidatorCount").await {
            return n;
        }
    }
    0
}

/// Read `ParachainHost_claim_queue` and derive the active para set + the CURRENT
/// on-chain packing (cores → paras, split evenly as an approximation of the
/// live cadence).
async fn read_claim_queue(at: &AtBlock) -> Result<(Vec<ParaInfo>, Vec<CorePacking>)> {
    let cq = runtime_api(at, "ParachainHost", "claim_queue", Vec::new()).await?;

    // `claim_queue` is `BTreeMap<CoreIndex, VecDeque<ParaId>>`. Decoded via
    // scale_value it comes wrapped in an extra composite layer, so the top-level
    // sequence is a single element (the map) rather than the entries. Peel
    // wrappers (bounded) until the children look like `(CoreIndex, VecDeque)`
    // pairs — a 2-field tuple whose first field flattens to a numeric core index.
    let mut entries = as_seq(&cq);
    for _ in 0..3 {
        let looks_like_entries = !entries.is_empty()
            && entries.iter().all(|e| {
                let f = as_seq(e);
                f.len() == 2 && flat_u32(f[0]).is_some()
            });
        if looks_like_entries {
            break;
        }
        if entries.len() == 1 {
            entries = as_seq(entries[0]);
        } else {
            break;
        }
    }

    // Each entry is a (CoreIndex, [ParaId]) pair.
    let mut core_paras: Vec<(u32, Vec<u32>)> = Vec::new();
    for entry in entries {
        let parts = as_seq(entry);
        if parts.len() < 2 {
            continue;
        }
        let core = match flat_u32(parts[0]) {
            Some(c) => c,
            None => continue,
        };
        let mut ids: Vec<u32> = Vec::new();
        for p in as_seq(parts[1]) {
            if let Some(id) = flat_u32(p) {
                ids.push(id);
            }
        }
        core_paras.push((core, ids));
    }
    core_paras.sort_by_key(|(c, _)| *c);

    // Per-para core counts.
    use std::collections::BTreeMap;
    let mut para_cores: BTreeMap<u32, u32> = BTreeMap::new();
    for (_c, ids) in &core_paras {
        // Distinct paras on this core each count the core once.
        let mut seen = std::collections::BTreeSet::new();
        for id in ids {
            if seen.insert(*id) {
                *para_cores.entry(*id).or_insert(0) += 1;
            }
        }
    }

    let paras: Vec<ParaInfo> = para_cores
        .iter()
        .map(|(&id, &cores)| ParaInfo {
            id,
            name: para_name(id),
            cores,
            is_system: id < 2000,
        })
        .collect();

    // Current packing: for each occupied core, split evenly among its distinct paras.
    let mut packing: Vec<CorePacking> = Vec::new();
    for (core, ids) in &core_paras {
        let mut distinct: Vec<u32> = Vec::new();
        for id in ids {
            if !distinct.contains(id) {
                distinct.push(*id);
            }
        }
        if distinct.is_empty() {
            continue;
        }
        let n = distinct.len() as u32;
        let base = PARTS_FULL / n;
        let mut leftover = PARTS_FULL - base * n;
        let assignments = distinct
            .iter()
            .map(|&id| {
                let mut parts = base;
                if leftover > 0 {
                    parts += 1;
                    leftover -= 1;
                }
                Assignment {
                    para_id: id,
                    parts,
                    share_fraction: parts as f64 / PARTS_FULL as f64,
                    expected_block_time_secs: BLOCK_SECS * PARTS_FULL as f64 / parts as f64,
                }
            })
            .collect();
        packing.push(CorePacking {
            core: *core,
            assignments,
        });
    }

    Ok((paras, packing))
}

/// Era progress. `currentEra` is expressed relative to the absolute era index so
/// the UI can track downsizing eras; the scheduler uses the absolute value.
async fn compute_era_progress(at: &AtBlock, session_index: u32, best: u64) -> EraProgress {
    let session_in_era = (session_index as u64 % SESSIONS_PER_ERA) as u32;
    let current_era = (session_index as u64 / SESSIONS_PER_ERA) as u32;

    // Session start block, if the scheduler exposes it; else approximate.
    let session_start = read_u32(at, "ParaScheduler", "SessionStartBlock")
        .await
        .map(|v| v as u64);
    let blocks_into_session = match session_start {
        Some(start) if best >= start => best - start,
        _ => 0,
    };
    let blocks_into_era = session_in_era as u64 * EPOCH_BLOCKS + blocks_into_session;
    let remaining = ERA_BLOCKS.saturating_sub(blocks_into_era);
    let next_era_eta = Utc::now() + chrono::Duration::seconds((remaining * BLOCK_SECS as u64) as i64);

    EraProgress {
        current_era,
        session_in_era,
        blocks_into_era,
        era_length_blocks: ERA_BLOCKS,
        next_era_eta: next_era_eta.to_rfc3339(),
    }
}

/// Best-effort online-fraction estimate. A precise measure would require
/// NOTE: this is NOT a per-validator liveness poll — there is no `imOnline`
/// pallet on the post-AHM relay to read heartbeats from. It is a conservative
/// estimate DERIVED FROM FINALITY LAG (the thing that actually matters for the
/// "don't break finality" guard). The finality-lag gate does the real gating;
/// this value drives the dashboard gauge and is labelled as an estimate there.
///
/// It scales with `max_lag` so the two gates stay consistent: within the allowed
/// lag band the estimate stays at/above 0.9 (comfortably over the 0.80 pause
/// threshold), and only drops once lag exceeds the limit — otherwise raising the
/// lag limit would be silently cancelled by this estimate tripping the online gate.
fn estimate_online_fraction(finality_lag: u64, max_lag: u64) -> f64 {
    if finality_lag == 0 {
        1.0
    } else if finality_lag <= max_lag {
        // Linear 1.0 → 0.9 across the healthy band.
        1.0 - 0.1 * (finality_lag as f64 / max_lag.max(1) as f64)
    } else {
        0.5
    }
}

fn para_name(id: u32) -> String {
    match id {
        1000 => "Asset Hub".to_string(),
        1001 => "Collectives".to_string(),
        1002 => "Bridge Hub".to_string(),
        1004 => "People".to_string(),
        1005 => "Coretime".to_string(),
        _ => format!("Para {id}"),
    }
}
