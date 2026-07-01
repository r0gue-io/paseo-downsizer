// Types that MATCH the SPEC.md "Service <-> UI API contract" JSON shapes exactly.
// Source of truth = on-chain state + planned tasks. See SPEC.md.
//
// Fields marked optional (`?`) are forward-compatible extensions the service MAY
// include (e.g. onlineFraction for the finality gauge); the SPEC shapes use `…`
// to indicate the objects are illustrative, so extra optional fields do not break
// the contract.

// ---------------------------------------------------------------------------
// GET /api/state — live chain snapshot (polled ~every 6s)
// ---------------------------------------------------------------------------

export type AhClientMode = "Active" | "Passive" | "Buffered" | string;

export interface EraProgress {
	currentEra: number;
	sessionInEra: number;
	blocksIntoEra: number;
	eraLengthBlocks: number;
	/** ISO-8601 UTC estimate of the next era boundary. */
	nextEraEta: string;
}

export interface ValidatorGroups {
	count: number;
	sizes: number[];
}

export interface RelayState {
	chain: string;
	specVersion: number;
	bestBlock: number;
	finalizedBlock: number;
	finalityLag: number;
	sessionIndex: number;
	eraProgress: EraProgress;
	validators: number;
	minValidatorSetSize: number;
	cores: number;
	ahClientMode: AhClientMode;
	validatorGroups: ValidatorGroups;
	/** Fraction (0..1) of the active set reporting online. Optional extension. */
	onlineFraction?: number;
}

export interface AssetHubState {
	chain: string;
	bestBlock: number;
	finalizedBlock: number;
	validatorCount: number;
}

export interface ParaInfo {
	id: number;
	name: string;
	cores: number;
	isSystem: boolean;
}

export interface CoreAssignment {
	paraId: number;
	parts: number;
	shareFraction: number;
	expectedBlockTimeSecs: number;
	/** Resolved chain name, when the service includes it. Optional extension. */
	name?: string;
}

export interface CorePacking {
	core: number;
	assignments: CoreAssignment[];
}

export interface ChainState {
	fetchedAt: string;
	relay: RelayState;
	assetHub: AssetHubState;
	paras: ParaInfo[];
	/** CURRENT on-chain packing. */
	packing: CorePacking[];
}

// ---------------------------------------------------------------------------
// GET /api/plan — schedule with per-step status derived from chain state
// ---------------------------------------------------------------------------

export type StepStatus =
	| "done"
	| "active"
	| "pending"
	| "paused"
	| "failed"
	| "shutdown";

export interface StepTargets {
	validators: number;
	cores: number;
	minSetSize: number;
}

export interface StepObserved {
	validators: number;
	cores: number;
}

export interface PlanStep {
	id: number;
	eraOffset: number;
	targets: StepTargets;
	status: StepStatus;
	scheduledEta: string;
	appliedAt: string | null;
	/** Same shape as state.packing — the packing this step will assign. */
	computedPacking: CorePacking[];
	observed?: StepObserved;
	note?: string;
	/** True for the terminal shutdown milestone (operators stop nodes). */
	shutdown: boolean;
}

export interface Plan {
	startedAt: string;
	eraHours: number;
	/** null when no step is active yet (or all steps are done). */
	currentStepId: number | null;
	/** Health gates the scheduler enforces (from plan `[health]`). */
	maxFinalityLagBlocks: number;
	minOnlineFraction: number;
	steps: PlanStep[];
}

// ---------------------------------------------------------------------------
// GET /api/history — dispatched txs
// ---------------------------------------------------------------------------

export type HistoryStatus =
	| "in_block"
	| "finalized"
	| "dry_run_failed"
	| "error"
	| "dry_run_ok";

export type DispatchChain = "relay" | "assetHub" | string;

export interface HistoryEntry {
	at: string;
	stepId: number;
	chain: DispatchChain;
	call: string;
	argsSummary: string;
	/** null for dry-run entries (no tx submitted). */
	txHash: string | null;
	status: HistoryStatus;
	blockHash: string | null;
	error: string | null;
}

export type History = HistoryEntry[];

// ---------------------------------------------------------------------------
// GET /api/health
// ---------------------------------------------------------------------------

export type HealthStatus = "ok" | "paused" | "error";

export interface Health {
	status: HealthStatus;
	reasons: string[];
	dispatcher: "armed" | "idle";
	lastError: string | null;
}

// ---------------------------------------------------------------------------
// POST /api/control
// ---------------------------------------------------------------------------

export interface ControlRequest {
	action: "pause" | "resume";
}
