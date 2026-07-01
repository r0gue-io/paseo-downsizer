// Runtime configuration read from NEXT_PUBLIC_* env vars (inlined at build time).

export const SERVICE_URL = (
	process.env.NEXT_PUBLIC_SERVICE_URL ?? "http://localhost:8080"
).replace(/\/$/, "");

/** Block explorer base for tx-hash links; empty string disables linking. */
export const EXPLORER_BASE = (process.env.NEXT_PUBLIC_EXPLORER ?? "").replace(
	/\/$/,
	"",
);

/** Poll interval for the service API, roughly one relay block. */
export const POLL_INTERVAL_MS = 6000;

// --- PAPI (chain-as-source-of-truth) toggle --------------------------------
export const USE_PAPI = process.env.NEXT_PUBLIC_USE_PAPI === "1";
export const RELAY_WS =
	process.env.NEXT_PUBLIC_RELAY_WS ?? "wss://paseo-rpc.n.dwellir.com";
export const AH_WS =
	process.env.NEXT_PUBLIC_AH_WS ?? "wss://asset-hub-paseo-rpc.n.dwellir.com";

export function explorerTxUrl(txHash: string | null): string | null {
	if (!EXPLORER_BASE || !txHash) return null;
	return `${EXPLORER_BASE}/${txHash}`;
}
