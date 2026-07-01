// PAPI (polkadot-api) client module — lets the UI treat the CHAIN as the source
// of truth for live counters (relay + Asset Hub), instead of only /api/state.
//
// Status: OPTIONAL / OFF BY DEFAULT.
//   The dashboard defaults to the service /api/state (see config USE_PAPI). To
//   enable direct chain reads set NEXT_PUBLIC_USE_PAPI=1 and ensure the
//   `polkadot-api` package is installed.
//
// Why no `papi add` descriptors are committed:
//   `papi add paseo -n paseo` + `papi add pah -n paseo_asset_hub` generate typed
//   descriptor bundles under `.papi/` from live metadata. That codegen needs
//   network access + a working `papi` CLI and is impractical to run in a build
//   sandbox, so we do NOT commit generated files. Instead we use the runtime
//   `client.getUnsafeApi()` escape hatch, which reads storage/constants/runtime
//   APIs dynamically off the live metadata — no codegen required. When you DO run
//   `papi add`, swap `getUnsafeApi()` for the typed `getTypedApi(descriptors)`.
//
// This module is written so the whole project type-checks even when
// `polkadot-api` is NOT installed: the package is imported via a non-literal
// dynamic specifier, so TypeScript does not try to resolve it at build time.

import { AH_WS, RELAY_WS } from "@/lib/config";

/* eslint-disable @typescript-eslint/no-explicit-any */
// biome-ignore lint/suspicious/noExplicitAny: dynamic (codegen-less) PAPI surface.
type AnyClient = any;

// Non-literal specifiers: prevents TS2307 when polkadot-api isn't installed and
// keeps the module tree-shakeable / lazy.
const PAPI_PKG = "polkadot-api";
const PAPI_WS_PKG = "polkadot-api/ws-provider/web";

let relayClient: AnyClient | null = null;
let ahClient: AnyClient | null = null;

async function createClientFor(wsUrl: string): Promise<AnyClient> {
	// biome-ignore lint/suspicious/noExplicitAny: dynamic import of optional dep.
	const papi: any = await import(/* webpackIgnore: true */ PAPI_PKG);
	// biome-ignore lint/suspicious/noExplicitAny: dynamic import of optional dep.
	const wsMod: any = await import(/* webpackIgnore: true */ PAPI_WS_PKG);
	const provider = wsMod.getWsProvider(wsUrl);
	return papi.createClient(provider);
}

export async function getRelayClient(): Promise<AnyClient> {
	if (!relayClient) relayClient = await createClientFor(RELAY_WS);
	return relayClient;
}

export async function getAssetHubClient(): Promise<AnyClient> {
	if (!ahClient) ahClient = await createClientFor(AH_WS);
	return ahClient;
}

export function destroyPapiClients(): void {
	relayClient?.destroy?.();
	ahClient?.destroy?.();
	relayClient = null;
	ahClient = null;
}

/** Live counters read directly from the relay chain via the unsafe (codeless) API. */
export interface RelayLiveCounters {
	bestBlock: number;
	finalizedBlock: number;
	validators: number;
	cores: number;
}

/**
 * Example live read using the codegen-less unsafe API. Kept intentionally small
 * and defensive — it is only exercised when NEXT_PUBLIC_USE_PAPI=1. Replace the
 * `getUnsafeApi()` calls with a typed API once `papi add` descriptors exist.
 */
export async function readRelayLiveCounters(): Promise<RelayLiveCounters> {
	const client = await getRelayClient();
	const api = client.getUnsafeApi();

	const [finalized, best, validators, config] = await Promise.all([
		client.getFinalizedBlock(),
		// biome-ignore lint/suspicious/noExplicitAny: codegen-less PAPI result shape.
		client
			.getBestBlocks()
			.then((b: any[]) => b[0]),
		api.query.Session.Validators.getValue(),
		api.query.Configuration.ActiveConfig.getValue(),
	]);

	return {
		bestBlock: Number(best?.number ?? 0),
		finalizedBlock: Number(finalized?.number ?? 0),
		validators: Array.isArray(validators) ? validators.length : 0,
		cores: Number(config?.scheduler_params?.num_cores ?? 0),
	};
}
