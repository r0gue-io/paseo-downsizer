"use client";

import { Server, Star } from "lucide-react";
import { useMemo, useState } from "react";
import { Badge } from "@/components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { fmtPct, fmtSecs } from "@/lib/format";
import type {
	ChainState,
	CoreAssignment,
	CorePacking,
	Plan,
} from "@/lib/types";
import { cn } from "@/lib/utils";

const ASSET_HUB_ID = 1000;
const CHART_VARS = [
	"var(--chart-2)",
	"var(--chart-1)",
	"var(--chart-3)",
	"var(--chart-4)",
	"var(--chart-5)",
];

interface Props {
	state: ChainState | null;
	plan: Plan | null;
}

export function CorePackingMap({ state, plan }: Props) {
	const [view, setView] = useState<"current" | "next">("current");

	const nameById = useMemo(() => {
		const m = new Map<number, string>();
		for (const p of state?.paras ?? []) m.set(p.id, p.name);
		return m;
	}, [state]);

	const nextStep = useMemo(() => {
		const steps = plan?.steps ?? [];
		return (
			steps.find((s) => s.status === "active") ??
			steps.find((s) => s.status === "pending") ??
			steps.find((s) => s.id === plan?.currentStepId) ??
			null
		);
	}, [plan]);

	const current = state?.packing ?? [];
	const next = nextStep?.computedPacking ?? [];
	const packing = view === "current" ? current : next;

	return (
		<Card className="h-full">
			<CardHeader className="flex-row items-start justify-between gap-4">
				<div className="grid gap-1.5">
					<CardTitle>Core packing map</CardTitle>
					<CardDescription>
						{view === "current"
							? "Current on-chain coretime assignments."
							: nextStep
								? `Computed packing for step ${nextStep.id} (${next.length} cores).`
								: "No upcoming step packing available."}
					</CardDescription>
				</div>
				<div className="bg-muted inline-flex shrink-0 items-center gap-1 rounded-lg p-1">
					{(["current", "next"] as const).map((v) => {
						const active = view === v;
						const isDisabled = v === "next" && next.length === 0;
						return (
							<button
								key={v}
								type="button"
								disabled={isDisabled}
								aria-pressed={active}
								onClick={() => setView(v)}
								className={cn(
									"h-7 rounded-md px-3 text-xs font-medium transition-colors",
									"focus-visible:ring-ring focus-visible:ring-2 focus-visible:outline-none",
									"disabled:pointer-events-none disabled:opacity-40",
									active
										? "bg-background text-foreground shadow-sm"
										: "text-muted-foreground hover:text-foreground",
								)}
							>
								{v === "current" ? "Current" : "Next step"}
							</button>
						);
					})}
				</div>
			</CardHeader>
			<CardContent>
				{!state ? (
					<div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
						{[0, 1, 2, 3, 4, 5].map((i) => (
							<Skeleton key={i} className="h-32 w-full" />
						))}
					</div>
				) : packing.length === 0 ? (
					<p className="text-muted-foreground text-sm">
						No packing data for this view.
					</p>
				) : (
					<div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
						{packing
							.slice()
							.sort((a, b) => a.core - b.core)
							.map((core) => (
								<CoreCard key={core.core} core={core} nameById={nameById} />
							))}
					</div>
				)}
			</CardContent>
		</Card>
	);
}

function CoreCard({
	core,
	nameById,
}: {
	core: CorePacking;
	nameById: Map<number, string>;
}) {
	const hasAssetHub = core.assignments.some((a) => a.paraId === ASSET_HUB_ID);
	const assignments = core.assignments
		.slice()
		.sort((a, b) => b.shareFraction - a.shareFraction);

	return (
		<div
			className={cn(
				"grid gap-2.5 rounded-lg border p-3",
				hasAssetHub && "border-chart-2/50 bg-chart-2/5",
			)}
		>
			<div className="flex items-center justify-between">
				<span className="flex items-center gap-1.5 text-sm font-medium">
					<Server className="text-muted-foreground size-3.5" />
					Core {core.core}
				</span>
				<span className="text-muted-foreground text-xs">
					{assignments.length} chain{assignments.length === 1 ? "" : "s"}
				</span>
			</div>

			{/* stacked share bar */}
			<div className="bg-muted flex h-2 w-full overflow-hidden rounded-full">
				{assignments.map((a, i) => (
					<div
						key={a.paraId}
						style={{
							width: `${a.shareFraction * 100}%`,
							background: CHART_VARS[i % CHART_VARS.length],
						}}
						title={`${label(a, nameById)} — ${fmtPct(a.shareFraction)}`}
					/>
				))}
			</div>

			<ul className="grid gap-1.5">
				{assignments.map((a, i) => (
					<li
						key={a.paraId}
						className="flex items-center justify-between gap-2 text-xs"
					>
						<span className="flex min-w-0 items-center gap-1.5">
							<span
								className="inline-block size-2 shrink-0 rounded-[3px]"
								style={{ background: CHART_VARS[i % CHART_VARS.length] }}
							/>
							<span className="truncate">{label(a, nameById)}</span>
							{a.paraId === ASSET_HUB_ID ? (
								<Badge
									variant="outline"
									className="border-chart-2/50 text-chart-2 gap-1 px-1 py-0"
								>
									<Star className="size-2.5" />
									AH
								</Badge>
							) : null}
						</span>
						<span className="text-muted-foreground shrink-0 font-mono tabular-nums">
							{fmtPct(a.shareFraction)} · {fmtSecs(a.expectedBlockTimeSecs)}
						</span>
					</li>
				))}
			</ul>
		</div>
	);
}

function label(a: CoreAssignment, nameById: Map<number, string>): string {
	return a.name ?? nameById.get(a.paraId) ?? `Para ${a.paraId}`;
}
