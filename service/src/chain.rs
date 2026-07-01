//! Long-lived subxt clients (relay + Asset Hub) with a reconnecting RPC backend,
//! plus small dynamic read helpers used to build `/api/state`.

use anyhow::{anyhow, Context, Result};
use subxt::dynamic::Value;
use subxt::config::RpcConfigFor;
use subxt::rpcs::client::reconnecting_rpc_client::RpcClient as ReconnectingRpcClient;
use subxt::rpcs::{LegacyRpcMethods, RpcClient};
use subxt::{OnlineClient, OnlineClientAtBlock, PolkadotConfig};

pub type Cfg = PolkadotConfig;
/// The subxt-rpcs config bridge for our chain config, needed by
/// [`LegacyRpcMethods`].
pub type RpcCfg = RpcConfigFor<Cfg>;
/// A subxt client scoped to a concrete block, on which storage/runtime-API/
/// constant reads are performed.
pub type AtBlock = OnlineClientAtBlock<Cfg>;

/// A connection to one chain: the dynamic subxt client plus the legacy RPC
/// methods (used for `system_dryRun`), sharing one reconnecting transport.
#[derive(Clone)]
pub struct ChainClient {
    pub label: &'static str,
    pub url: String,
    pub online: OnlineClient<Cfg>,
    pub legacy: LegacyRpcMethods<RpcCfg>,
}

impl ChainClient {
    /// Connect to `url`. The underlying reconnecting RPC client transparently
    /// re-establishes the websocket on transient failures.
    pub async fn connect(label: &'static str, url: &str) -> Result<Self> {
        let reconnecting = ReconnectingRpcClient::builder()
            .build(url.to_string())
            .await
            .with_context(|| format!("[{label}] building reconnecting rpc client for {url}"))?;
        let rpc_client = RpcClient::new(reconnecting);
        let online = OnlineClient::<Cfg>::from_rpc_client(rpc_client.clone())
            .await
            .with_context(|| format!("[{label}] subxt online client for {url}"))?;
        let legacy = LegacyRpcMethods::<RpcCfg>::new(rpc_client);
        tracing::info!(target: "chain", "[{label}] connected to {url}");
        Ok(ChainClient {
            label,
            url: url.to_string(),
            online,
            legacy,
        })
    }

    /// Reconnect from scratch (used when a subscription dies hard).
    pub async fn reconnect(&mut self) -> Result<()> {
        let fresh = ChainClient::connect(self.label, &self.url).await?;
        self.online = fresh.online;
        self.legacy = fresh.legacy;
        Ok(())
    }

    /// A client scoped to the current finalized block.
    pub async fn at_current(&self) -> Result<AtBlock> {
        Ok(self.online.at_current_block().await?)
    }
}

/// Fetch a plain (keyless) storage entry at a block, decoded to a dynamic
/// `Value`. Returns `None` if the entry has no value.
pub async fn fetch_storage(at: &AtBlock, pallet: &str, entry: &str) -> Result<Option<Value>> {
    let maybe = at
        .storage()
        .try_fetch((pallet, entry), Vec::<Value>::new())
        .await
        .with_context(|| format!("fetch {pallet}.{entry}"))?;
    match maybe {
        Some(sv) => Ok(Some(sv.decode()?)),
        None => Ok(None),
    }
}

/// Call a runtime API method, returning its decoded dynamic `Value`.
pub async fn runtime_api(
    at: &AtBlock,
    trait_name: &str,
    method: &str,
    args: Vec<Value>,
) -> Result<Value> {
    let payload = subxt::dynamic::runtime_api_call::<Vec<Value>, Value>(
        trait_name.to_string(),
        method.to_string(),
        args,
    );
    let res = at
        .runtime_apis()
        .call(payload)
        .await
        .with_context(|| format!("runtime api {trait_name}_{method}"))?;
    Ok(res)
}

/// Read a `u128`-shaped constant, if present.
pub fn constant_u128(at: &AtBlock, pallet: &str, name: &str) -> Result<u128> {
    let v: Value = at
        .constants()
        .entry((pallet, name))
        .with_context(|| format!("constant {pallet}.{name}"))?;
    v.as_u128()
        .ok_or_else(|| anyhow!("constant {pallet}.{name} is not an integer"))
}
