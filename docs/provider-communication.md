# Paseo Relay Downsizing & Sunset — Provider & User Communication

**Status:** planned · **Audience:** Paseo validators, collators, parachain teams, and users
**Owner:** Paseo maintainers (r0gue) · **Live dashboard:** _(paseo-downsizer UI URL)_

---

## TL;DR

The current Paseo relay chain is being **wound down and shut off** as we launch a
purpose-built replacement chain. **It starts Thursday 2 July 2026, 12:00 CEST
(10:00 UTC); the old relay is shut down ~48h later, Saturday 4 July, 12:00 CEST.**
The whole process takes about **48 hours**:

1. **Downsize (~24h):** validators drop from **152 → 20** in four steps (one per
   6h era); cores drop **56 → 4**. This frees infrastructure for the new chain.
2. **Grace window (~24h):** the chain holds at **20 validators / 4 cores** so you
   can finish migrating to the new chain. All parachains stay alive (slower).
3. **Shutdown (~T+48h):** validator operators **stop their nodes**. The chain
   halts. **The old relay is gone after this point.**

- **During downsizing + grace: no relay downtime, finality never breaks.**
  Parachains keep producing blocks at a **reduced cadence** (~24–60s).
- **Asset Hub keeps a dedicated core** and near-normal block time throughout.
- **Action required:** **migrate to the new chain before T+48h.** After shutdown
  the old relay stops permanently.

Watch it live on the dashboard — every number comes from on-chain state.

---

## Why

We are replacing the current relay with a right-sized chain launched in parallel.
Our total infrastructure budget is 80 nodes (validators + collators + RPCs) across
both chains, so the old chain must shrink quickly to free capacity for the new one
— while staying alive just long enough for a supervised migration, then shutting
down cleanly.

---

## Schedule

Membership changes only at **era boundaries (every 6h)**, so we take **one step
per era**. Group size (validators per backing group) is held at **5** the whole
way, so backing/approval/dispute security is maintained at every step.

| Phase | When (approx) | Validators | Cores | What it means |
|------|---------------|-----------:|------:|---------------|
| Baseline | now | 152 | 56 | normal |
| Downsize 1 | T+6h | 100 | 20 | negligible; consolidation begins |
| Downsize 2 | T+12h | 60 | 12 | parachain block times start to lengthen |
| Downsize 3 | T+18h | 40 | 8 | further consolidation |
| Downsize 4 | T+24h | **20** | 4 | **floor reached** |
| **Grace / migrate** | T+24h → T+48h | 20 | 4 | chain held alive for final migration |
| **Shutdown** | **T+48h** | — | — | **operators stop nodes; chain halts** |

_The grace window can be extended to 48h (shutdown at T+72h) if needed — a
one-line change. The dashboard always shows the live, current schedule._

### Parachain cadence during downsizing

All ~31 active parachains are packed onto the shrinking core set (time-shared),
so nothing goes dark — chains just produce blocks more slowly:

| Chain class | At the floor (4 cores) | Approx block time |
|-------------|------------------------|-------------------|
| **Asset Hub** | dedicated core | ~6–12s (near-normal) |
| Bridge Hub, People, Coretime, Collectives | share a core | ~30–48s |
| Community parachains | ~8 per core | ~48–60s |

---

## Impact & (non-)downtime

| Who | Impact | Action needed |
|-----|--------|---------------|
| **Everyone** | Relay keeps finalizing normally through downsizing + grace. No outage until the scheduled shutdown. | Migrate to the new chain before **T+48h** |
| **Parachain teams** | Your chain keeps producing at a **slower, steadier cadence** during downsizing. At **T+48h it stops permanently** with the relay. | **Redeploy / register on the new chain before shutdown.** Contact us if you need a larger core share or more time |
| **Parachain users** | Slower confirmations on affected chains; Asset Hub near-normal. Old chain unusable after T+48h. | Move activity to the new chain before shutdown |
| **Validators staying (the floor 20)** | You remain in the active set to the end; backing groups get slightly larger. At shutdown you stop your node on the coordinated signal. | Keep your node healthy; **stop it at the shutdown signal** |
| **Validators being offboarded** | **Chilled first (no slashing)**, then rotate out at an era boundary. After that your node is no longer an authority. | We contact your cohort with timing. Do **not** stop your node until we confirm it has rotated out |
| **RPC / infra operators** | Old-chain endpoints serve until shutdown, then can be decommissioned / repointed to the new chain. | Plan endpoint cutover for T+48h |

**No unplanned downtime.** The only scheduled outage is the **deliberate shutdown
at T+48h**, which is the whole point — the old chain ends there.

---

## The shutdown ("going to zero")

There is no on-chain "stop the network" transaction — a chain runs as long as its
validators run. So the final step is **operational**: at the shutdown milestone,
validator operators **stop their nodes** on a coordinated signal, block production
and finality cease, and the chain is effectively at **zero validators = halted**.
The service **schedules and announces** this milestone (with the exact block/time
on the dashboard); it does not — and cannot — stop operators' nodes for them.

## Safety summary

- Validator removals happen **only at era boundaries**; offboarded nodes are
  **chilled (never slashed)** and decommissioned only **after** they leave the set.
- Finality (GRANDPA) needs >2/3 of the active set online; group size stays at 5,
  and the schedule **auto-pauses** if finality lag or online headroom degrades.
- Every governance call is **dry-run before submission** and applied **atomically**
  (one `utility.batch_all` per chain) so each step lands in a single block.

## Contact

Questions, a request for a larger core share, or more migration time: reach the
Paseo maintainers via the usual channels _(add Matrix/Element + email here)_.
Offboarding validators and the floor-20 cohort will be contacted directly with
per-cohort timing and the shutdown signal.
