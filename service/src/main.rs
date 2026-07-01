//! paseo-downsizer service: monitors the relay + Asset Hub, computes each
//! downsizing step, and auto-dispatches it via `proxy.proxy(sudo, sudo.sudo(...))`
//! at the scheduled era, while serving the JSON API the UI consumes.

mod api;
mod chain;
mod config;
mod dispatch;
mod matrix;
mod model;
mod packing;
mod providers;
mod scheduler;
mod shared;
mod state;
mod state_store;
mod valueutil;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use subxt::utils::AccountId32;
use subxt_signer::sr25519::Keypair;
use subxt_signer::SecretUri;

use crate::chain::ChainClient;
use crate::config::{Plan, Settings};
use crate::dispatch::Dispatcher;
use crate::scheduler::{RunMode, Scheduler};
use crate::shared::Shared;
use crate::state_store::Persisted;

#[derive(Parser, Debug)]
#[command(name = "paseo-downsizer-service", about = "Paseo relay downsizing dispatcher + API")]
struct Cli {
    /// Path to the downsizing plan (defaults to env PLAN_PATH, then a sensible relative path).
    #[arg(long, env = "PLAN_PATH")]
    plan: Option<PathBuf>,

    /// Run a single scheduler tick, then exit.
    #[arg(long)]
    once: bool,

    /// Simulate (dry-run) the next step without submitting, then exit.
    #[arg(long)]
    dry_run: bool,

    /// Post the REAL per-cycle offboarding notice(s) to Matrix as a clearly-marked
    /// PREVIEW, then exit. No chain connection, no dispatch — use to smoke-test the
    /// notifier and show providers exactly what to expect. Needs providers.toml +
    /// MATRIX_* env, same as the live notifier.
    #[arg(long)]
    matrix_test: bool,

    /// With --matrix-test, limit the preview to a single cycle (1..4).
    /// Default: preview every cycle that has tagged providers leaving.
    #[arg(long)]
    matrix_test_cycle: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let plan_path = cli
        .plan
        .clone()
        .unwrap_or_else(|| PathBuf::from("../plan/downsizing-plan.toml"));
    let plan = Plan::load(&plan_path)
        .with_context(|| format!("loading plan from {}", plan_path.display()))?;
    let plan = Arc::new(plan);
    let settings = Settings::from_env(&plan);

    tracing::info!(target: "main", "plan '{}' loaded: {} enabled step(s), era={}h, soak={} era(s)",
        plan.meta.name, plan.enabled_steps().len(), plan.meta.era_hours, plan.meta.soak_eras);

    // --matrix-test: render the real cycle notice(s) as a marked PREVIEW and post
    // them, then exit. Deliberately BEFORE any chain connect — it touches no chain
    // and dispatches nothing.
    if cli.matrix_test {
        return run_matrix_test(&plan, &plan_path, &settings, cli.matrix_test_cycle).await;
    }

    // Connect both chains (reconnecting transport).
    let relay = ChainClient::connect("relay", &settings.relay_ws)
        .await
        .context("connecting to relay")?;
    let ah = ChainClient::connect("asset-hub", &settings.ah_ws)
        .await
        .context("connecting to Asset Hub")?;

    // Build the dispatcher if we have the proxy key + sudo account; otherwise
    // run in monitor-only mode (API + state, no dispatch).
    let dispatcher = build_dispatcher(&relay, &ah, &plan, &settings).await?;
    let armed = dispatcher.is_some();
    if !armed {
        tracing::warn!(target: "main",
            "monitor-only mode: PROXY_SURI and/or SUDO_ACCOUNT not set — no calls will be dispatched");
    }

    // Persisted history + progress.
    let state_path = PathBuf::from("state.json");
    let persisted = Persisted::load(&state_path);
    let shared = Shared::new(
        plan.clone(),
        persisted,
        settings.control_token.clone(),
        state_path,
        armed,
    );

    let mode = if cli.dry_run {
        RunMode::DryRunOnce
    } else if cli.once {
        RunMode::Once
    } else {
        RunMode::Auto
    };

    // Optional Matrix notifier: needs the local providers.toml + MATRIX_* env.
    let providers = load_providers(&plan_path);
    let matrix = matrix::Matrix::connect(
        settings.matrix_homeserver.clone(),
        settings.matrix_token.clone(),
        settings.matrix_room.clone(),
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(target: "main", "matrix connect failed: {e:#}");
        None
    });
    match (&matrix, &providers) {
        (Some(_), Some(_)) => tracing::info!(target: "main", "matrix notifier enabled"),
        _ => tracing::info!(target: "main", "matrix notifier disabled (needs MATRIX_* env + providers.toml)"),
    }

    let scheduler = Scheduler {
        shared: shared.clone(),
        relay,
        ah,
        dispatcher,
        mode,
        matrix,
        providers,
    };

    match mode {
        RunMode::Auto => {
            // Serve the API and run the scheduler concurrently.
            let app = api::router(shared.clone());
            let addr = std::net::SocketAddr::from_str(&settings.bind_addr)
                .with_context(|| format!("parsing BIND_ADDR {}", settings.bind_addr))?;
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .with_context(|| format!("binding {addr}"))?;
            tracing::info!(target: "main", "API listening on http://{addr}");

            let server = tokio::spawn(async move {
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!(target: "main", "api server error: {e:#}");
                }
            });

            let sched = tokio::spawn(async move {
                if let Err(e) = scheduler.run().await {
                    tracing::error!(target: "main", "scheduler stopped: {e:#}");
                }
            });

            tokio::select! {
                _ = server => tracing::warn!(target: "main", "api server task ended"),
                _ = sched => tracing::warn!(target: "main", "scheduler task ended"),
                _ = tokio::signal::ctrl_c() => tracing::info!(target: "main", "shutdown requested"),
            }
        }
        RunMode::Once | RunMode::DryRunOnce => {
            tracing::info!(target: "main", "running one-shot mode: {:?}", mode);
            scheduler.run().await?;
        }
    }

    Ok(())
}

/// Load the local provider directory (`providers.toml`) from `PROVIDERS_PATH` or
/// next to the plan. Returns `None` (notifications disabled) if absent/unparseable.
fn load_providers(plan_path: &std::path::Path) -> Option<providers::Providers> {
    std::env::var("PROVIDERS_PATH")
        .map(PathBuf::from)
        .ok()
        .or_else(|| plan_path.parent().map(|d| d.join("providers.toml")))
        .filter(|p| p.exists())
        .and_then(|p| match providers::Providers::load(&p) {
            Ok(pr) => {
                tracing::info!(target: "main", "loaded provider directory: {} providers", pr.providers.len());
                Some(pr)
            }
            Err(e) => {
                tracing::warn!(target: "main", "providers.toml load failed: {e:#}");
                None
            }
        })
}

/// Render the real per-cycle offboarding notice(s) and post them to Matrix as a
/// clearly-marked PREVIEW, then return. Exercises the exact live path
/// (`providers.cycle_notice` + `Matrix::post`) but touches no chain and dispatches
/// nothing. `only` limits to a single cycle id.
async fn run_matrix_test(
    plan: &Plan,
    plan_path: &std::path::Path,
    settings: &Settings,
    only: Option<u32>,
) -> Result<()> {
    let providers = load_providers(plan_path).ok_or_else(|| {
        anyhow::anyhow!(
            "providers.toml not found/loadable (next to the plan or via PROVIDERS_PATH); \
             cannot render cycle notices"
        )
    })?;
    let matrix = matrix::Matrix::connect(
        settings.matrix_homeserver.clone(),
        settings.matrix_token.clone(),
        settings.matrix_room.clone(),
    )
    .await
    .context("connecting Matrix for --matrix-test")?
    .ok_or_else(|| {
        anyhow::anyhow!("Matrix not configured — set MATRIX_HOMESERVER, MATRIX_TOKEN, MATRIX_ROOM")
    })?;

    // Unmistakable preview banner so the room never mistakes this for a live event.
    let banner_text = "🧪 PREVIEW / SMOKE TEST — NOT a live downsizing event. This is a \
        sample of the notice you'll receive when this cycle actually executes.\n\n";
    let banner_html = "<b>🧪 PREVIEW / SMOKE TEST — NOT a live downsizing event.</b> This is a \
        sample of the notice you'll receive when this cycle actually executes.<br/><br/>";

    let mut posted = 0u32;
    for (cycle, from, to) in plan.preview_cycles() {
        if only.is_some_and(|c| c != cycle) {
            continue;
        }
        match providers.cycle_notice(cycle, from, to) {
            Some((text, html)) => {
                matrix
                    .post(&format!("{banner_text}{text}"), &format!("{banner_html}{html}"))
                    .await
                    .with_context(|| format!("posting preview for cycle {cycle}"))?;
                tracing::info!(target: "matrix-test", "posted preview for cycle {cycle} ({from} → {to})");
                posted += 1;
            }
            None => tracing::info!(
                target: "matrix-test",
                "cycle {cycle}: no tagged providers leave — nothing to preview"
            ),
        }
    }
    if posted == 0 {
        tracing::warn!(target: "matrix-test",
            "no preview posted (cycle {:?} has no tagged providers, or no cycles matched)", only);
    } else {
        tracing::info!(target: "matrix-test", "✓ posted {posted} preview notice(s) — check the room");
    }
    Ok(())
}

async fn build_dispatcher(
    relay: &ChainClient,
    ah: &ChainClient,
    plan: &Plan,
    settings: &Settings,
) -> Result<Option<Dispatcher>> {
    let (Some(suri), Some(sudo_ss58)) = (&settings.proxy_suri, &settings.sudo_account) else {
        return Ok(None);
    };

    let uri = SecretUri::from_str(suri).context("parsing PROXY_SURI")?;
    let signer = Keypair::from_uri(&uri).context("deriving keypair from PROXY_SURI")?;
    let sudo: AccountId32 = sudo_ss58
        .parse()
        .with_context(|| format!("parsing SUDO_ACCOUNT ss58 {sudo_ss58}"))?;

    let dispatcher = Dispatcher::new(relay, ah, signer, sudo, plan.dispatch.dry_run_first)
        .await
        .context("initializing dispatcher (metadata resolution)")?;
    Ok(Some(dispatcher))
}
