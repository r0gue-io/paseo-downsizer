# paseo-downsizer — build contract

Authoritative spec for the two deliverables. Build agents MUST follow this so the
Rust service and the TypeScript UI interoperate. The single source of truth at
runtime is **on-chain state + `plan/downsizing-plan.toml`**; nothing is hardcoded
that can be read from the chain.

## What this system does

Orchestrates and visualizes the controlled downsizing of the **live Paseo relay**:
shrinks the validator set (152 → 20 over ~24h) and the core count (56 → 4) one
step per era (6h), **packing all active parachains onto the shrinking core set**
so no ecosystem chain goes dark — they just produce blocks at a lower pace —
then holds at the floor for a ~24h grace window and finally **shuts the chain
down** (a terminal, non-dispatched milestone: validator operators stop nodes).
See `plan/downsizing-plan.toml` and `docs/` for the plan.

## Repo layout (fixed — each deliverable owns its subtree)

```
plan/downsizing-plan.toml      # authored, do not regenerate
docs/provider-communication.md # authored, do not regenerate
SPEC.md  README.md             # authored
service/                       # Rust (subxt + axum) — dispatch agent owns this
ui/                            # Next.js 16 + PAPI — ui agent owns this
```

## Chain facts (verified live 2026-07-01, spec 2_003_001)

- Relay: `Paseo Testnet`, ss58=0, 10 decimals, 6s blocks. Session/epoch = 600
  blocks = **1h**; `SessionsPerEra = 6` → **era = 6h**. Membership changes only
  at era boundaries.
- Relay is **post-AHM**: `stakingAhClient.mode = Active`; the validator set is
  elected on **Asset Hub** by `pallet_staking_async`. `session.validators` on the
  relay = current active set (152).
- `MinimumValidatorSetSize` is a **dynamic param, default 100**, settable via
  relay `parameters.set_parameter(RuntimeParameters::AhClient(MinimumValidatorSetSize(n)))`
  under Root. The `parameters` map is currently empty (default in force).
- Cores: `configuration.activeConfig.schedulerParams.numCores = 56`. Backing
  groups = `paraScheduler.validatorGroups` (56 groups sized 2–3).
- Active paras: read from runtime API `ParachainHost_claim_queue` (map
  core→[paraId]) and/or `paraScheduler.coreDescriptors`. ~31 active; Asset Hub
  (1000) spans 3 cores.

## The three on-chain levers (dispatched as `proxy.proxy(sudo, sudo.sudo(inner))`)

Every dispatch is `Proxy.proxy { real: SUDO_ACCOUNT, force_proxy_type: None,
call: Sudo.sudo { call: <inner> } }`, signed by the **proxy delegate key**
(`PROXY_SURI`). Route each to the correct chain:

| Lever | Chain | Inner call (resolve exact name from metadata at startup) |
|---|---|---|
| Validator count | Asset Hub | `staking_async.set_validator_count(n)` (a.k.a. staking set_validator_count) |
| Exit cohort | Asset Hub | `staking_async.force_unstake(stash, 0)` per exit stash — Root-safe (verified); `chill_other` needs Signed so can't run via sudo |
| Min set size | Relay | `parameters.set_parameter(AhClient::MinimumValidatorSetSize(n))` |
| Core count | Relay | `coretime.request_core_count(n)` (runtime_parachains coretime) |
| Packing | Relay | `coretime.assign_core(core, begin, [(CoreAssignment::Task(paraId), PartsOf57600)...], end_hint=None)` |

Rules:
- Resolve pallet/call indices and field names from **live metadata** (subxt);
  do not assume static indices — the live runtime is ahead of tag v2.3.1.
- **Ordering guard:** `min_validator_set_size` for a step must be applied
  (relay) before the AH `validatorCount` that would produce a set below the old
  min, or the set is dropped (`SetTooSmallAndDropped`) and the reduction silently
  no-ops. The plan sets min=40 in step 1 to satisfy this for all later steps.
- **Packing math:** for each target core, the `PartsOf57600` across its assigned
  paras MUST sum to exactly 57600. `begin` = next timeslice boundary
  (`now / TIMESLICE_PERIOD + 1`, TIMESLICE_PERIOD from constants; live = 80).
- **Dry-run** every inner call via `system_dryRun`/`DryRunApi.dry_run_call`
  before submit; if `dry_run_first` and any call fails, abort the whole step.

## Packing algorithm (deterministic — service implements)

Input: live active paras `P`, target `cores C`, `weights` + `default_weight` +
`max_chains_per_core` + `hard_max_chains_per_core` from the plan.
1. Weight each para (`weights[id]` or `default_weight`).
2. Sort paras by weight desc. Asset Hub (1000, highest weight) gets its own core
   if a core is available (never share it below 1/2 of a core).
3. Greedily bin-pack remaining paras onto the remaining cores, ≤
   `max_chains_per_core` per core; if paras don't fit, raise the per-core cap
   toward `hard_max_chains_per_core`; if still short, log that some low-weight
   paras are dropped to on-demand (never silently — surface in the API).
4. On each core, split 57600 proportional to the weights of its paras (largest
   remainder rounding so the sum is exactly 57600).
5. Produce `assign_core` calls. Expose the computed plan via the API so the UI
   can render "chain → core → share → expected block time" (block time ≈
   6s / share_fraction).

## Service ↔ UI API contract (axum HTTP+JSON, served by the Rust service)

CORS open to the UI origin. All timestamps ISO-8601 UTC. Endpoints:

- `GET /api/state` → live chain snapshot (polled ~every 6s):
  ```jsonc
  {
    "fetchedAt": "…",
    "relay": { "chain": "Paseo Testnet", "specVersion": 2003001,
               "bestBlock": 12345678, "finalizedBlock": 12345676,
               "finalityLag": 2, "sessionIndex": 20699,
               "eraProgress": { "currentEra": 0, "sessionInEra": 3,
                 "blocksIntoEra": 1234, "eraLengthBlocks": 3600,
                 "nextEraEta": "…" },
               "validators": 152, "minValidatorSetSize": 100,
               "cores": 56, "ahClientMode": "Active",
               "validatorGroups": { "count": 56, "sizes": [3,3,2,…] } },
    "assetHub": { "chain": "…", "bestBlock": …, "finalizedBlock": …,
                  "validatorCount": 152 },
    "paras": [ { "id": 1000, "name": "Asset Hub", "cores": 3,
                 "isSystem": true } … ],
    "packing": [ { "core": 0, "assignments": [ { "paraId": 1000,
                   "parts": 57600, "shareFraction": 1.0,
                   "expectedBlockTimeSecs": 6 } ] } … ]  // CURRENT on-chain packing
  }
  ```
- `GET /api/plan` → the schedule with per-step status derived from chain state:
  ```jsonc
  {
    "startedAt": "…", "eraHours": 6, "currentStepId": 2,
    "steps": [ { "id": 1, "eraOffset": 1,
                 "targets": { "validators": 100, "cores": 20, "minSetSize": 40 },
                 "status": "done|active|pending|paused|failed",
                 "scheduledEta": "…", "appliedAt": "…|null",
                 "computedPacking": [ /* same shape as state.packing */ ],
                 "observed": { "validators": 100, "cores": 20 } } … ]
  }
  ```
- `GET /api/history` → dispatched txs: `[{ "at":"…", "stepId":1, "chain":"relay",
  "call":"parameters.set_parameter", "argsSummary":"…", "txHash":"0x…",
  "status":"in_block|finalized|dry_run_failed|error", "blockHash":"0x…",
  "error": null }]`.
- `GET /api/health` → `{ "status":"ok|paused|error", "reasons":[…],
  "dispatcher":"armed|idle", "lastError": null }`.
- `POST /api/control` → `{ "action":"pause"|"resume" }` (guard for operators even
  in auto mode). Auth: bearer token from env `CONTROL_TOKEN` (optional).

The service is authoritative for `/api/plan` (intent + schedule + dispatch
history). The UI reads `/api/*` for the plan/history AND may additionally read
the chain directly via PAPI for live counters — but on-chain state shown must
reconcile with `/api/state`.

## Rust service requirements (`service/`)

- Edition 2021. `subxt` (dynamic API off live metadata — no codegen files
  committed) + `tokio` + `axum` + `serde` + `toml` + `tracing` + `sp-core`/`subxt-signer`
  for the SURI. Persist history + progress to a local `state.json` (or sqlite via
  `rusqlite`; JSON is fine).
- Config from env + `plan/downsizing-plan.toml` (path via `--plan` / env
  `PLAN_PATH`). Env: `PROXY_SURI` (delegate seed), `SUDO_ACCOUNT` (ss58 of the
  real sudo key the proxy acts for), `RELAY_WS`, `AH_WS` (override plan),
  `CONTROL_TOKEN` (optional), `BIND_ADDR` (default 127.0.0.1:8080).
- Two long-lived subxt clients (relay + AH) with reconnect.
- **Scheduler loop:** on each new relay block, recompute era progress; when the
  active step's era boundary is reached AND soak satisfied AND health OK, build
  the step's calls (min-size → core-count → packing on relay; validatorCount +
  force_unstakes on AH), dry-run all, then submit via proxy+sudo. Record to history.
  Advance `currentStepId` only after the chain reflects the targets.
- **Idempotent / crash-safe:** derive current step from observed chain state on
  startup; never re-issue a target already met. Re-assert packing if the broker
  overwrote it (log when it does).
- `--once`/`--dry-run` CLI flags for a full dry-run of the next step without
  submitting. `cargo check` MUST pass (verify phase runs it).

## UI requirements (`ui/`) — match paseo-website exactly

- Next.js 16 (App Router, RSC) + React 19 + TypeScript + Tailwind v4 +
  **shadcn/ui "new-york", base color neutral, CSS variables** + `lucide-react` +
  `next-themes` (system default, light+dark) + **Geist / Geist Mono** fonts +
  **Biome** (not eslint/prettier). pnpm. Mirror `globals.css` design tokens from
  `/home/alemart/Projects/paseo-network/paseo-website/src/app/globals.css`
  (neutral zinc surfaces, indigo/blue chart-1..5, radius 0.625rem, shadow scale).
  Copy that token block verbatim into `ui/src/app/globals.css`.
- **PAPI** (`polkadot-api`) for live chain reads client-side (relay + AH
  descriptors generated via `papi add`); the service `/api/*` for plan + history.
  If PAPI codegen is impractical in the build sandbox, fall back to reading
  everything from the service `/api/state`, but keep a PAPI client module wired
  so it can be enabled — the UI MUST be able to treat the chain as source of truth.
- Single dashboard (`/`), everything dynamic, auto-refresh (~6s / block). Panels:
  1. **Headline progress** — validators now→target with a progress bar, cores
     now→target, current step + countdown to next era boundary (live ticking).
  2. **Timeline** — the steps as a vertical stepper: done / active / pending,
     each with target validators+cores, ETA, and applied-at.
  3. **Finality health** — best vs finalized, finality lag, ah_client mode,
     online-fraction gauge with the 2/3 danger line marked.
  4. **Core packing map** — per core, the chains sharing it and each chain's
     share % + expected block time; highlight Asset Hub. Show current vs the
     next step's computed packing.
  5. **Dispatch log** — `/api/history` table with tx hashes (link to an explorer
     if configured) and dry-run/finalized status.
- Charts: lightweight (recharts or plain SVG) using the chart-1..5 CSS vars.
- `SERVICE_URL` from `NEXT_PUBLIC_SERVICE_URL` (default `http://localhost:8080`).
- `pnpm build` (or `pnpm tsc --noEmit`) MUST pass in the verify phase.

## Non-goals / guard rails

- Never write private keys to disk or repo. `PROXY_SURI` from env only.
- Never break finality: the scheduler must refuse to dispatch if `finalityLag >
  max_finality_lag_blocks` or online fraction below `min_online_fraction`.
- Never decommission logic here — the service only reduces the elected set +
  cores + packing; physically powering off rotated-out nodes is an operator step
  documented in `docs/provider-communication.md`.
