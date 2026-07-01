//! Deterministic core-packing algorithm (SPEC "Packing algorithm").
//!
//! Pure functions only — no chain access. Input is the live active-para set +
//! the plan's weights; output is a per-core assignment where the `PartsOf57600`
//! on each core sum to exactly 57600.

use crate::config::Packing;
use crate::model::{Assignment, CorePacking};

pub const PARTS_FULL: u32 = 57600;
pub const BLOCK_SECS: f64 = 6.0;
pub const ASSET_HUB_ID: u32 = 1000;

/// A para considered for packing.
#[derive(Debug, Clone)]
pub struct Para {
    pub id: u32,
    pub weight: u32,
}

/// Result of packing: the per-core plan plus any paras that could not be placed
/// (they must fall back to on-demand — surfaced, never silently dropped).
#[derive(Debug, Clone)]
pub struct PackingResult {
    pub cores: Vec<CorePacking>,
    pub dropped: Vec<u32>,
}

/// Compute the packing for `target_cores` cores over the live `para_ids`.
pub fn compute_packing(para_ids: &[u32], target_cores: u32, cfg: &Packing) -> PackingResult {
    let mut dropped = Vec::new();

    if target_cores == 0 {
        return PackingResult {
            cores: Vec::new(),
            dropped: para_ids.to_vec(),
        };
    }

    // 1. Weight each para.
    let mut paras: Vec<Para> = para_ids
        .iter()
        .map(|&id| Para {
            id,
            weight: cfg.weight_for(id),
        })
        .collect();

    // 2. Sort by weight desc, then id asc for determinism.
    paras.sort_by(|a, b| b.weight.cmp(&a.weight).then(a.id.cmp(&b.id)));

    // Buckets: one Vec<Para> per core.
    let mut buckets: Vec<Vec<Para>> = vec![Vec::new(); target_cores as usize];

    // 2/3. Asset Hub gets its own dedicated core if one is available and there is
    // at least one other core for everyone else (never share AH below 1/2 a core).
    let mut ah_core: Option<usize> = None;
    if target_cores >= 2 {
        if let Some(pos) = paras.iter().position(|p| p.id == ASSET_HUB_ID) {
            let ah = paras.remove(pos);
            buckets[0].push(ah);
            ah_core = Some(0);
        }
    }

    // Cores available for the remaining paras.
    let packable_cores: Vec<usize> = (0..target_cores as usize)
        .filter(|c| Some(*c) != ah_core)
        .collect();

    // 3. Determine the per-core chain cap. Start at max_chains_per_core; raise
    // toward hard_max_chains_per_core until everyone fits, if possible.
    let n = paras.len();
    let r = packable_cores.len().max(1);
    let mut cap = cfg.max_chains_per_core.max(1);
    while (n as u32) > (r as u32) * cap && cap < cfg.hard_max_chains_per_core {
        cap += 1;
    }

    // Greedy bin-pack: highest weight first into the least-loaded core (by total
    // weight) that still has capacity. Deterministic tie-break: lowest core index.
    for para in paras.into_iter() {
        let mut best: Option<usize> = None;
        let mut best_load: u32 = u32::MAX;
        for &c in &packable_cores {
            if buckets[c].len() as u32 >= cap {
                continue;
            }
            let load: u32 = buckets[c].iter().map(|p| p.weight).sum();
            if load < best_load {
                best_load = load;
                best = Some(c);
            }
        }
        match best {
            Some(c) => buckets[c].push(para),
            // No core has capacity even at hard_max: fall back to on-demand.
            None => dropped.push(para.id),
        }
    }

    // 4. Split 57600 per core proportional to weights (largest remainder).
    let mut cores = Vec::new();
    for (idx, bucket) in buckets.iter().enumerate() {
        if bucket.is_empty() {
            continue;
        }
        let parts = split_parts(bucket);
        let assignments = bucket
            .iter()
            .zip(parts.iter())
            .map(|(p, &parts)| Assignment {
                para_id: p.id,
                parts,
                share_fraction: parts as f64 / PARTS_FULL as f64,
                expected_block_time_secs: if parts == 0 {
                    f64::INFINITY
                } else {
                    BLOCK_SECS * PARTS_FULL as f64 / parts as f64
                },
            })
            .collect();
        cores.push(CorePacking {
            core: idx as u32,
            assignments,
        });
    }

    dropped.sort_unstable();
    PackingResult { cores, dropped }
}

/// Split `PARTS_FULL` across a bucket proportional to weights using largest
/// remainder rounding so the parts sum to exactly `PARTS_FULL`.
fn split_parts(bucket: &[Para]) -> Vec<u32> {
    let total_weight: u64 = bucket.iter().map(|p| p.weight as u64).sum();
    if total_weight == 0 {
        // Degenerate: split evenly.
        return even_split(bucket.len());
    }

    // Floor of the exact proportional share, and its fractional remainder.
    let mut floors: Vec<u32> = Vec::with_capacity(bucket.len());
    let mut remainders: Vec<(usize, u64)> = Vec::with_capacity(bucket.len());
    let mut assigned: u32 = 0;
    for (i, p) in bucket.iter().enumerate() {
        let exact = PARTS_FULL as u64 * p.weight as u64; // /total_weight later
        let floor = (exact / total_weight) as u32;
        let rem = exact % total_weight;
        floors.push(floor);
        remainders.push((i, rem));
        assigned += floor;
    }

    // Distribute the leftover parts to the largest remainders (tie-break: index).
    let mut leftover = PARTS_FULL.saturating_sub(assigned);
    remainders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut k = 0;
    while leftover > 0 && !remainders.is_empty() {
        let (i, _) = remainders[k % remainders.len()];
        floors[i] += 1;
        leftover -= 1;
        k += 1;
    }
    floors
}

fn even_split(n: usize) -> Vec<u32> {
    if n == 0 {
        return Vec::new();
    }
    let base = PARTS_FULL / n as u32;
    let mut out = vec![base; n];
    let mut leftover = PARTS_FULL - base * n as u32;
    let mut i = 0;
    while leftover > 0 {
        out[i] += 1;
        leftover -= 1;
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn cfg() -> Packing {
        let mut weights = BTreeMap::new();
        weights.insert("1000".to_string(), 100);
        weights.insert("1002".to_string(), 20);
        Packing {
            max_chains_per_core: 6,
            hard_max_chains_per_core: 10,
            default_weight: 5,
            weights,
        }
    }

    #[test]
    fn each_core_sums_to_full() {
        let paras: Vec<u32> = vec![1000, 1002, 1004, 1005, 1001, 1500, 1501, 1502, 2000, 2001];
        let res = compute_packing(&paras, 6, &cfg());
        for core in &res.cores {
            let sum: u32 = core.assignments.iter().map(|a| a.parts).sum();
            assert_eq!(sum, PARTS_FULL, "core {} parts must sum to full", core.core);
        }
    }

    #[test]
    fn asset_hub_gets_dedicated_core() {
        let paras: Vec<u32> = vec![1000, 1002, 1004];
        let res = compute_packing(&paras, 6, &cfg());
        let ah_core = res
            .cores
            .iter()
            .find(|c| c.assignments.iter().any(|a| a.para_id == ASSET_HUB_ID))
            .unwrap();
        assert_eq!(ah_core.assignments.len(), 1);
        assert_eq!(ah_core.assignments[0].parts, PARTS_FULL);
    }
}
