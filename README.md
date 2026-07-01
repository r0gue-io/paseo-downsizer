# paseo-downsizer

Monitor + auto-dispatch service and live dashboard for the **controlled downsizing
and sunset of the Paseo relay chain**: shrink the validator set (152 → 20 over
~24h) and the core schedule (56 → 4) one step per era (6h), **packing all active
parachains onto the shrinking core set** so no chain goes dark — they just produce
blocks at a lower cadence — then hold at the floor for a ~24h grace window and
**shut the chain down** (validator operators stop nodes).

Finality-safe by construction: steps land only at era boundaries, the schedule
soaks and auto-pauses on any finality/health breach, and offboarded validators are
cleanly removed (never slashed) and only decommissioned after they rotate out.

## Components

| Path | What | Stack |
|------|------|-------|
| `service/` | Monitors relay + Asset Hub, computes each step, and **auto-dispatches** it via `proxy.proxy(sudo, sudo.sudo(...))` at the scheduled era. Serves the JSON API. | Rust — subxt + axum + tokio |
| `ui/` | Live dashboard: progress, step timeline, finality health, per-core packing map, dispatch log. Source of truth = on-chain state + planned tasks. | Next.js 16 + React 19 + Tailwind v4 + shadcn/ui + **PAPI** |
| `plan/downsizing-plan.toml` | The schedule: targets per step + packing weights. Source of truth for intent. | — |
| `docs/provider-communication.md` | The plan to communicate to providers & users (timelines, impact, non-downtime). | — |
| `SPEC.md` | Build contract (chain facts, the three levers, packing algorithm, API). | — |

## The three on-chain levers

All dispatched as `proxy.proxy(real = sudo, sudo.sudo(inner))`, signed by a proxy
delegate key (`PROXY_SURI`, env only). A **SafeSudo** delegate suffices.

1. **Validators** — Asset Hub `staking_async.set_validator_count` (+
   `force_unstake` per exit-cohort stash to force exactly who leaves; Root-safe).
   Post-AHM, the set is elected on AH.
2. **Min set size** — relay `parameters.set_parameter(AhClient::MinimumValidatorSetSize)`
   (dynamic param, default 100). Lowered first so sub-100 sets aren't dropped.
3. **Cores + packing** — relay `coretime.request_core_count` and
   `coretime.assign_core` (time-share paras across the remaining cores).

## Run

```bash
# service
cd service
export RELAY_WS=wss://paseo-rpc.n.dwellir.com
export AH_WS=wss://asset-hub-paseo-rpc.n.dwellir.com
export SUDO_ACCOUNT=<sudo-account-ss58>     # ss58 of the sudo key the proxy acts for
export PROXY_SURI="…"                        # delegate seed/mnemonic — env only, NEVER commit
cargo run -- --plan ../plan/downsizing-plan.toml            # add --dry-run to simulate the next step only

# ui
cd ui
export NEXT_PUBLIC_SERVICE_URL=http://localhost:8080
pnpm install && pnpm dev
```

## Safety

- `dry_run_first = true`: every call is dry-run; a failing step aborts before submit.
- Auto-pause on `finalityLag > 10` or online fraction `< 0.80`.
- Idempotent + crash-safe: current step is derived from observed chain state on
  startup; targets already met are never re-issued.
- Never decommission nodes from here — that's an operator step (see `docs/`).

See `SPEC.md` for the full contract and `plan/downsizing-plan.toml` for the schedule.

## License

[Apache-2.0](LICENSE).
