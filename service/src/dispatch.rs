//! The three on-chain levers, dispatched as
//! `Proxy.proxy(real = SUDO_ACCOUNT, force_proxy_type = None, Sudo.sudo(inner))`,
//! signed by the proxy delegate key. Inner calls are built dynamically and the
//! pallet/call names are resolved from live metadata (SPEC "The three on-chain
//! levers").

use anyhow::{anyhow, Context, Result};
use subxt::dynamic::Value;
use subxt::utils::AccountId32;
use subxt_signer::sr25519::Keypair;

use crate::chain::{runtime_api, ChainClient};
use crate::config::{Plan, Step};
use crate::model::CorePacking;
use crate::packing::PARTS_FULL;
use crate::valueutil::{as_seq, field, flat_u128, variant_name};

/// XCM version passed to `DryRunApi.dry_run_call` (the V2 signature takes a
/// `result_xcms_version`). Paseo relay + AH expose this runtime API.
const DRY_RUN_XCM_VERSION: u32 = 4;

/// Which chain an item is dispatched to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainKind {
    Relay,
    AssetHub,
}

impl ChainKind {
    pub fn api_name(&self) -> &'static str {
        match self {
            ChainKind::Relay => "relay",
            ChainKind::AssetHub => "assetHub",
        }
    }
}

/// Pallet/call names resolved from metadata at startup (logged).
#[derive(Debug, Clone)]
pub struct Resolved {
    pub proxy_pallet: String,
    pub sudo_pallet: String,
    // relay
    pub parameters_pallet: String,
    pub coretime_pallet: String,
    pub utility_pallet: String,
    // asset hub
    pub ah_proxy_pallet: String,
    pub ah_sudo_pallet: String,
    pub staking_pallet: String,
    pub set_validator_count_call: String,
    pub chill_other_call: String,
    pub ah_utility_pallet: String,
    /// The live timeslice period (constant), used to compute `assign_core` begin.
    pub timeslice_period: u32,
}

/// A single inner call, already wrapped as a proxy+sudo transaction and ready to
/// dry-run / submit on `chain`.
pub struct DispatchItem {
    pub chain: ChainKind,
    pub call_path: String,
    pub args_summary: String,
    /// The proxy+sudo-wrapped call as a `RuntimeCall` value, used for per-item
    /// dry-run via `DryRunApi.dry_run_call` (which takes a decoded call, not
    /// signed bytes — and, unlike `system_dryRun`, is a safe public state-call).
    /// Actual submission goes through the atomic `BatchTx` (see `batch`).
    pub call_value: Value,
    /// The innermost `RuntimeCall` (e.g. `Parameters.set_parameter(..)`), before
    /// the sudo/proxy wrapping — collected into a `Utility.batch_all` so a whole
    /// step lands atomically in one block.
    pub inner: Value,
}

/// A whole chain's step wrapped as one atomic
/// `Proxy.proxy(real, None, Sudo.sudo(Utility.batch_all([calls])))`.
pub struct BatchTx {
    pub chain: ChainKind,
    pub call_value: Value,
    pub payload: subxt::tx::DynamicPayload<Vec<Value>>,
    pub count: usize,
}

pub struct Dispatcher {
    pub signer: Keypair,
    pub sudo: AccountId32,
    pub resolved: Resolved,
    /// Whether every call must be dry-run before submit (from the plan).
    pub dry_run_first: bool,
}

impl Dispatcher {
    /// Whether dry-run-before-submit is required.
    pub fn requires_dry_run(&self) -> bool {
        self.dry_run_first
    }
}

impl Dispatcher {
    /// Resolve pallet/call names from live metadata and log them.
    pub async fn new(
        relay: &ChainClient,
        ah: &ChainClient,
        signer: Keypair,
        sudo: AccountId32,
        dry_run_first: bool,
    ) -> Result<Self> {
        let relay_at = relay.at_current().await?;
        let relay_md = relay_at.metadata();
        let ah_at = ah.at_current().await?;
        let ah_md = ah_at.metadata();

        let proxy_pallet = pallet_with_call(&relay_md, "proxy").unwrap_or_else(|| "Proxy".into());
        let sudo_pallet = pallet_with_call(&relay_md, "sudo").unwrap_or_else(|| "Sudo".into());
        let parameters_pallet =
            pallet_with_call(&relay_md, "set_parameter").unwrap_or_else(|| "Parameters".into());
        let coretime_pallet = pallet_with_call(&relay_md, "request_core_count")
            .unwrap_or_else(|| "Coretime".into());
        let utility_pallet =
            pallet_with_call(&relay_md, "batch_all").unwrap_or_else(|| "Utility".into());

        let ah_proxy_pallet = pallet_with_call(&ah_md, "proxy").unwrap_or_else(|| "Proxy".into());
        let ah_sudo_pallet = pallet_with_call(&ah_md, "sudo").unwrap_or_else(|| "Sudo".into());
        let staking_pallet = pallet_with_call(&ah_md, "set_validator_count")
            .unwrap_or_else(|| "Staking".into());
        let set_validator_count_call = "set_validator_count".to_string();
        let chill_other_call = "chill_other".to_string();
        let ah_utility_pallet =
            pallet_with_call(&ah_md, "batch_all").unwrap_or_else(|| "Utility".into());

        // TIMESLICE_PERIOD from coretime constants (live = 80).
        let timeslice_period = crate::chain::constant_u128(&relay_at, &coretime_pallet, "TimeslicePeriod")
            .ok()
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(80);

        let resolved = Resolved {
            proxy_pallet,
            sudo_pallet,
            parameters_pallet,
            coretime_pallet,
            utility_pallet,
            ah_proxy_pallet,
            ah_sudo_pallet,
            staking_pallet,
            set_validator_count_call,
            chill_other_call,
            ah_utility_pallet,
            timeslice_period,
        };

        tracing::info!(target: "dispatch",
            "resolved metadata calls: relay proxy={}.proxy sudo={}.sudo params={}.set_parameter coretime={}.{{request_core_count,assign_core}}; AH proxy={}.proxy sudo={}.sudo staking={}.{{{},{}}}; timeslice_period={}",
            resolved.proxy_pallet, resolved.sudo_pallet, resolved.parameters_pallet, resolved.coretime_pallet,
            resolved.ah_proxy_pallet, resolved.ah_sudo_pallet, resolved.staking_pallet,
            resolved.set_validator_count_call, resolved.chill_other_call, resolved.timeslice_period);

        Ok(Dispatcher {
            signer,
            sudo,
            resolved,
            dry_run_first,
        })
    }

    // ---- inner call builders (dynamic Values) ----

    /// A RuntimeCall value: `Pallet(Call(fields...))`, positional (unnamed) so we
    /// never depend on field-name stability.
    fn runtime_call(pallet: &str, call: &str, fields: Vec<Value>) -> Value {
        Value::unnamed_variant(
            pallet.to_string(),
            [Value::unnamed_variant(call.to_string(), fields)],
        )
    }

    /// Wrap an inner RuntimeCall in `Sudo.sudo(call)`.
    fn sudo_wrap(&self, chain: ChainKind, inner: Value) -> Value {
        let sudo_pallet = match chain {
            ChainKind::Relay => &self.resolved.sudo_pallet,
            ChainKind::AssetHub => &self.resolved.ah_sudo_pallet,
        };
        Self::runtime_call(sudo_pallet, "sudo", vec![inner])
    }

    /// Wrap a `Sudo.sudo(...)` RuntimeCall in `Proxy.proxy(real, None, call)` and
    /// return the top-level submittable payload.
    fn proxy_wrap(
        &self,
        chain: ChainKind,
        sudo_call: Value,
    ) -> (Value, subxt::tx::DynamicPayload<Vec<Value>>) {
        let proxy_pallet = match chain {
            ChainKind::Relay => &self.resolved.proxy_pallet,
            ChainKind::AssetHub => &self.resolved.ah_proxy_pallet,
        };
        let real = Value::unnamed_variant("Id", [account_value(&self.sudo)]);
        let force_proxy_type = Value::unnamed_variant("None", Vec::<Value>::new());
        let fields = vec![real, force_proxy_type, sudo_call];
        // The RuntimeCall value (for DryRunApi) and the submittable payload share
        // the same argument list.
        let call_value = Self::runtime_call(proxy_pallet, "proxy", fields.clone());
        let payload =
            subxt::dynamic::tx(proxy_pallet.to_string(), "proxy".to_string(), fields);
        (call_value, payload)
    }

    fn wrap(
        &self,
        chain: ChainKind,
        call_path: String,
        args_summary: String,
        inner: Value,
    ) -> DispatchItem {
        let item_inner = inner.clone();
        let sudo_call = self.sudo_wrap(chain, inner);
        // Per-item we only need the call value (for dry-run diagnostics); the
        // submittable payload is built once per chain by `batch`.
        let (call_value, _payload) = self.proxy_wrap(chain, sudo_call);
        DispatchItem {
            chain,
            call_path,
            args_summary,
            call_value,
            inner: item_inner,
        }
    }

    /// The `Utility` pallet name for a chain.
    fn utility_pallet(&self, chain: ChainKind) -> &str {
        match chain {
            ChainKind::Relay => &self.resolved.utility_pallet,
            ChainKind::AssetHub => &self.resolved.ah_utility_pallet,
        }
    }

    /// Combine a chain's step items into ONE atomic transaction:
    /// `Proxy.proxy(real, None, Sudo.sudo(Utility.batch_all([inner calls])))`.
    /// `batch_all` is all-or-nothing, so the whole reconfiguration lands (or
    /// reverts) in a single block — no transient state on the live network.
    pub fn batch(&self, chain: ChainKind, items: &[&DispatchItem]) -> Option<BatchTx> {
        if items.is_empty() {
            return None;
        }
        let calls: Vec<Value> = items.iter().map(|it| it.inner.clone()).collect();
        let batch_call = Self::runtime_call(
            self.utility_pallet(chain),
            "batch_all",
            vec![Value::unnamed_composite(calls)],
        );
        let sudo_call = self.sudo_wrap(chain, batch_call);
        let (call_value, payload) = self.proxy_wrap(chain, sudo_call);
        Some(BatchTx {
            chain,
            call_value,
            payload,
            count: items.len(),
        })
    }

    // ---- the levers ----

    fn min_set_size_item(&self, n: u32) -> DispatchItem {
        // set_parameter(RuntimeParameters::AhClient(MinimumValidatorSetSize(key, Some(n))))
        // The dynamic-params variant is a 2-field tuple: the unit-struct KEY
        // marker (`MinimumValidatorSetSize;`, encodes to zero bytes → empty
        // composite) followed by the `Option<u32>` VALUE. Sending only the value
        // fails encoding ("expected length 2 but got length 1").
        let key_marker = Value::unnamed_composite(Vec::<Value>::new());
        let value = Value::unnamed_variant("Some", [Value::u128(n as u128)]);
        let leaf = Value::unnamed_variant("MinimumValidatorSetSize", [key_marker, value]);
        let ah_client = Value::unnamed_variant("AhClient", [leaf]);
        let inner = Self::runtime_call(&self.resolved.parameters_pallet, "set_parameter", vec![ah_client]);
        self.wrap(
            ChainKind::Relay,
            format!("{}.set_parameter", self.resolved.parameters_pallet),
            format!("AhClient::MinimumValidatorSetSize({n})"),
            inner,
        )
    }

    fn core_count_item(&self, cores: u32) -> DispatchItem {
        let inner = Self::runtime_call(
            &self.resolved.coretime_pallet,
            "request_core_count",
            vec![Value::u128(cores as u128)],
        );
        self.wrap(
            ChainKind::Relay,
            format!("{}.request_core_count", self.resolved.coretime_pallet),
            format!("count={cores}"),
            inner,
        )
    }

    /// One `assign_core` call for a single core's packing.
    fn assign_core_item(&self, begin: u32, core: &CorePacking) -> DispatchItem {
        // assignment: Vec<(CoreAssignment::Task(paraId), PartsOf57600)>
        let assignments: Vec<Value> = core
            .assignments
            .iter()
            .map(|a| {
                let task = Value::unnamed_variant("Task", [Value::u128(a.para_id as u128)]);
                Value::unnamed_composite(vec![task, Value::u128(a.parts as u128)])
            })
            .collect();
        let inner = Self::runtime_call(
            &self.resolved.coretime_pallet,
            "assign_core",
            vec![
                Value::u128(core.core as u128),                 // core: CoreIndex
                Value::u128(begin as u128),                     // begin: BlockNumber
                Value::unnamed_composite(assignments),          // Vec<(CoreAssignment, PartsOf57600)>
                Value::unnamed_variant("None", Vec::<Value>::new()), // end_hint: Option
            ],
        );
        let summary = core
            .assignments
            .iter()
            .map(|a| format!("{}:{}", a.para_id, a.parts))
            .collect::<Vec<_>>()
            .join(",");
        self.wrap(
            ChainKind::Relay,
            format!("{}.assign_core", self.resolved.coretime_pallet),
            format!("core={} begin={} [{}]", core.core, begin, summary),
            inner,
        )
    }

    /// Public single-core `assign_core` builder for packing re-assertion.
    pub fn assign_core_reassert(&self, begin: u32, core: &CorePacking) -> DispatchItem {
        self.assign_core_item(begin, core)
    }

    fn validator_count_item(&self, n: u32) -> DispatchItem {
        let inner = Self::runtime_call(
            &self.resolved.staking_pallet,
            &self.resolved.set_validator_count_call,
            vec![Value::u128(n as u128)],
        );
        self.wrap(
            ChainKind::AssetHub,
            format!("{}.{}", self.resolved.staking_pallet, self.resolved.set_validator_count_call),
            format!("new={n}"),
            inner,
        )
    }

    fn chill_other_item(&self, stash: &AccountId32) -> DispatchItem {
        let inner = Self::runtime_call(
            &self.resolved.staking_pallet,
            &self.resolved.chill_other_call,
            vec![account_value(stash)],
        );
        self.wrap(
            ChainKind::AssetHub,
            format!("{}.{}", self.resolved.staking_pallet, self.resolved.chill_other_call),
            format!("stash={stash}"),
            inner,
        )
    }

    /// Build all dispatch items for a step. Relay order matters: min-size FIRST
    /// (so sub-old-min sets aren't dropped), then core-count, then packing. AH:
    /// validatorCount then chills.
    pub fn build_step_items(
        &self,
        plan: &Plan,
        step: &Step,
        packing: &[CorePacking],
        begin_timeslice: u32,
    ) -> Result<(Vec<DispatchItem>, Vec<DispatchItem>)> {
        // Sanity: each core's parts must sum to exactly 57600.
        for core in packing {
            let sum: u32 = core.assignments.iter().map(|a| a.parts).sum();
            if sum != PARTS_FULL {
                return Err(anyhow!(
                    "packing invariant violated: core {} parts sum to {} (expected {})",
                    core.core,
                    sum,
                    PARTS_FULL
                ));
            }
        }

        let mut relay = vec![
            self.min_set_size_item(step.min_validator_set_size),
            self.core_count_item(step.cores),
        ];
        for core in packing {
            relay.push(self.assign_core_item(begin_timeslice, core));
        }

        let mut ah = vec![self.validator_count_item(step.validators)];
        for stash_ss58 in plan.exit_cohort_for(step) {
            let stash: AccountId32 = stash_ss58
                .parse()
                .with_context(|| format!("parsing exit-cohort stash {stash_ss58}"))?;
            ah.push(self.chill_other_item(&stash));
        }

        Ok((relay, ah))
    }

    /// Dry-run one item via the `DryRunApi.dry_run_call` runtime API. Unlike the
    /// legacy `system_dryRun` RPC (unsafe, not exposed on public nodes), this is
    /// a safe state-call. The call is dispatched with the proxy delegate's signed
    /// origin — so it also validates the proxy relationship and the sudo path.
    /// Returns Ok(()) if the call would dispatch successfully.
    pub async fn dry_run(&self, chain: &ChainClient, item: &DispatchItem) -> Result<()> {
        self.dry_run_value(chain, &item.call_value, &item.call_path)
            .await
    }

    /// Dry-run a whole atomic batch (the transaction that will actually be
    /// submitted). Validates the all-or-nothing execution as one unit.
    pub async fn dry_run_batch(&self, chain: &ChainClient, batch: &BatchTx) -> Result<()> {
        self.dry_run_value(chain, &batch.call_value, "Utility.batch_all")
            .await
    }

    async fn dry_run_value(
        &self,
        chain: &ChainClient,
        call_value: &Value,
        path: &str,
    ) -> Result<()> {
        let at = chain.at_current().await?;
        // origin = OriginCaller::system(RawOrigin::Signed(delegate))
        let who = AccountId32(self.signer.public_key().0);
        let origin = Value::unnamed_variant(
            "system",
            [Value::unnamed_variant("Signed", [account_value(&who)])],
        );
        let args = vec![
            origin,
            call_value.clone(),
            Value::u128(DRY_RUN_XCM_VERSION as u128),
        ];
        let res = runtime_api(&at, "DryRunApi", "dry_run_call", args)
            .await
            .context("DryRunApi.dry_run_call")?;
        interpret_dry_run(&res, path)
    }

    /// Submit an atomic batch and wait for finalization. Returns (tx, block).
    pub async fn submit_batch(
        &self,
        chain: &ChainClient,
        batch: &BatchTx,
    ) -> Result<(String, String)> {
        self.submit_payload(chain, &batch.payload).await
    }

    async fn submit_payload(
        &self,
        chain: &ChainClient,
        payload: &subxt::tx::DynamicPayload<Vec<Value>>,
    ) -> Result<(String, String)> {
        let mut txc = chain.online.tx().await?;
        let progress = txc
            .sign_and_submit_then_watch_default(payload, &self.signer)
            .await
            .context("sign and submit")?;
        let tx_hash = format!("{:?}", progress.extrinsic_hash());
        let in_block = progress
            .wait_for_finalized()
            .await
            .context("waiting for finalization")?;
        let block_hash = format!("{:?}", in_block.block_hash());
        in_block
            .wait_for_success()
            .await
            .context("extrinsic dispatched with error")?;
        Ok((tx_hash, block_hash))
    }
}

/// Encode an ss58 account as a dynamic value (unnamed composite of 32 bytes),
/// which scale-encodes into `AccountId32`.
fn account_value(acc: &AccountId32) -> Value {
    Value::from_bytes(acc.0)
}

/// Interpret a `DryRunApi.dry_run_call` result: `Result<CallDryRunEffects, Error>`
/// where `CallDryRunEffects.execution_result` is itself a dispatch `Result`.
/// Ok(()) only when both the runtime API and the inner dispatch succeeded.
fn interpret_dry_run(res: &Value, path: &str) -> Result<()> {
    match variant_name(res) {
        Some("Ok") => {
            let effects = as_seq(res)
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("dry-run Ok had no effects for {path}"))?;
            let exec = field(effects, "execution_result").ok_or_else(|| {
                anyhow!("dry-run effects missing execution_result for {path}")
            })?;
            match variant_name(exec) {
                Some("Ok") => Ok(()),
                Some("Err") => Err(anyhow!(
                    "dry-run dispatch error for {path}: {}",
                    describe(exec)
                )),
                _ => Err(anyhow!("dry-run execution_result not a Result for {path}")),
            }
        }
        Some("Err") => Err(anyhow!(
            "dry-run rejected (DryRunApi Err) for {path}: {}",
            describe(res)
        )),
        _ => Err(anyhow!("dry-run result not a Result variant for {path}")),
    }
}

/// Best-effort rendering of a nested error value as a `::`-joined variant chain
/// (e.g. `Err::Module::Proxy::NotProxy`), for readable dry-run diagnostics.
fn describe(v: &Value) -> String {
    let mut parts = Vec::new();
    let mut cur = v;
    for _ in 0..10 {
        if let Some(name) = variant_name(cur) {
            parts.push(name.to_string());
            // Module errors carry the offending pallet index + error byte(s),
            // nested inside a `ModuleError { index, error }` struct.
            if name == "Module" {
                let me = as_seq(cur).into_iter().next().unwrap_or(cur);
                if let Some(idx) = field(me, "index").and_then(flat_u128) {
                    parts.push(format!("pallet#{idx}"));
                }
                if let Some(err) = field(me, "error") {
                    let b: Vec<String> = as_seq(err)
                        .iter()
                        .filter_map(|x| flat_u128(x))
                        .map(|n| n.to_string())
                        .collect();
                    if !b.is_empty() {
                        parts.push(format!("err=[{}]", b.join(",")));
                    }
                }
                break;
            }
            // Prefer an `error` field (DispatchErrorWithPostInfo) else first child.
            match field(cur, "error").or_else(|| as_seq(cur).into_iter().next()) {
                Some(next) => cur = next,
                None => break,
            }
        } else {
            // Composite: follow `error`, else descend a single-child wrapper.
            match field(cur, "error") {
                Some(next) => cur = next,
                None => {
                    let seq = as_seq(cur);
                    if seq.len() == 1 {
                        cur = seq[0];
                    } else {
                        break;
                    }
                }
            }
        }
    }
    if parts.is_empty() {
        "unknown".to_string()
    } else {
        parts.join("::")
    }
}

/// The name of the first pallet exposing a call named `call_name`.
fn pallet_with_call(md: &subxt::Metadata, call_name: &str) -> Option<String> {
    for pallet in md.pallets() {
        if pallet.call_variant_by_name(call_name).is_some() {
            return Some(pallet.name().to_string());
        }
    }
    None
}

/// The next timeslice boundary: `now / TIMESLICE_PERIOD + 1`.
pub fn next_timeslice(now_block: u64, timeslice_period: u32) -> u32 {
    let tp = timeslice_period.max(1) as u64;
    ((now_block / tp) + 1) as u32
}
