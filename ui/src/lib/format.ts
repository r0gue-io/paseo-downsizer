// Pure formatting/derivation helpers. Deterministic — no LLM, no side effects.

import type { HistoryStatus, StepStatus } from "@/lib/types";

export function fmtInt(n: number | null | undefined): string {
	if (n == null || !Number.isFinite(n)) return "—";
	return new Intl.NumberFormat("en-US").format(n);
}

export function fmtPct(
	fraction: number | null | undefined,
	digits = 1,
): string {
	if (fraction == null || !Number.isFinite(fraction)) return "—";
	return `${(fraction * 100).toFixed(digits)}%`;
}

export function fmtSecs(secs: number | null | undefined): string {
	if (secs == null || !Number.isFinite(secs)) return "—";
	if (secs < 1) return `${(secs * 1000).toFixed(0)}ms`;
	if (secs < 60) return `${secs % 1 === 0 ? secs : secs.toFixed(1)}s`;
	const m = Math.floor(secs / 60);
	const s = Math.round(secs % 60);
	return `${m}m ${s}s`;
}

export function shortHash(
	hash: string | null | undefined,
	lead = 6,
	tail = 4,
): string {
	if (!hash) return "—";
	if (hash.length <= lead + tail + 2) return hash;
	return `${hash.slice(0, lead)}…${hash.slice(-tail)}`;
}

export function fmtTime(iso: string | null | undefined): string {
	if (!iso) return "—";
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return "—";
	return d.toLocaleString(undefined, {
		month: "short",
		day: "2-digit",
		hour: "2-digit",
		minute: "2-digit",
		second: "2-digit",
	});
}

/** "3s ago", "5m ago", etc. */
export function fmtRelative(ms: number | null | undefined): string {
	if (ms == null) return "never";
	const delta = Date.now() - ms;
	if (delta < 0) return "just now";
	const s = Math.floor(delta / 1000);
	if (s < 2) return "just now";
	if (s < 60) return `${s}s ago`;
	const m = Math.floor(s / 60);
	if (m < 60) return `${m}m ago`;
	const h = Math.floor(m / 60);
	return `${h}h ago`;
}

/** Progress fraction from a shrinking count: 0 at start, 1 at target. */
export function reductionProgress(
	start: number,
	current: number,
	target: number,
): number {
	if (start === target) return 1;
	const p = (start - current) / (start - target);
	return Math.max(0, Math.min(1, p));
}

export type BadgeVariant = "default" | "secondary" | "destructive" | "outline";

export function stepStatusVariant(status: StepStatus): BadgeVariant {
	switch (status) {
		case "done":
			return "secondary";
		case "active":
			return "default";
		case "failed":
			return "destructive";
		case "shutdown":
			return "destructive";
		case "paused":
			return "outline";
		default:
			return "outline";
	}
}

export function historyStatusVariant(status: HistoryStatus): BadgeVariant {
	switch (status) {
		case "finalized":
			return "default";
		case "in_block":
		case "dry_run_ok":
			return "secondary";
		case "dry_run_failed":
		case "error":
			return "destructive";
		default:
			return "outline";
	}
}
