"use client";

import { Boxes, Cpu, Hourglass, Layers } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import { useCountdown } from "@/hooks/useCountdown";
import { fmtInt, reductionProgress } from "@/lib/format";
import type { ChainState, Plan } from "@/lib/types";

interface Props {
	state: ChainState | null;
	plan: Plan | null;
}

export function HeadlineProgress({ state, plan }: Props) {
	const countdown = useCountdown(state?.relay.eraProgress.nextEraEta ?? null);
	const golive = useCountdown(plan?.startsAt ?? null);
	const waiting = !!plan?.startsAt && !!golive && !golive.elapsed;

	if (!state) {
		return (
			<Card>
				<CardHeader>
					<CardTitle>Downsizing progress</CardTitle>
				</CardHeader>
				<CardContent className="grid gap-6 md:grid-cols-2 xl:grid-cols-4">
					{[0, 1, 2, 3].map((i) => (
						<Skeleton key={i} className="h-24 w-full" />
					))}
				</CardContent>
			</Card>
		);
	}

	const steps = plan?.steps ?? [];
	const lastStep = steps.length > 0 ? steps[steps.length - 1] : null;
	const currentStep =
		steps.find((s) => s.id === plan?.currentStepId) ??
		steps.find((s) => s.status === "active") ??
		null;

	const nowVals = state.relay.validators;
	const nowCores = state.relay.cores;
	const targetVals = lastStep?.targets.validators ?? nowVals;
	const targetCores = lastStep?.targets.cores ?? nowCores;

	const baseVals = Math.max(nowVals, ...steps.map((s) => s.targets.validators));
	const baseCores = Math.max(nowCores, ...steps.map((s) => s.targets.cores));

	return (
		<Card>
			<CardHeader className="flex-row items-center justify-between gap-4">
				<div className="grid gap-1.5">
					<CardTitle>Downsizing progress</CardTitle>
					<CardDescription>
						Relay validator set and core schedule, one step per era (
						{plan?.eraHours ?? 6}h).
					</CardDescription>
				</div>
				{waiting ? (
					<Badge variant="secondary" className="gap-1.5">
						<Hourglass className="size-3 animate-pulse" />
						Armed · go-live in {golive?.label}
					</Badge>
				) : (
					<Badge
						variant={currentStep ? "default" : "outline"}
						className="gap-1.5"
					>
						<Layers className="size-3" />
						{currentStep
							? `Step ${currentStep.id} · ${currentStep.status}`
							: "No active step"}
					</Badge>
				)}
			</CardHeader>
			<CardContent className="grid gap-6 md:grid-cols-2 xl:grid-cols-4">
				<Metric
					icon={<Boxes className="size-4" />}
					label="Validators"
					now={nowVals}
					target={targetVals}
					progress={reductionProgress(baseVals, nowVals, targetVals)}
				/>
				<Metric
					icon={<Cpu className="size-4" />}
					label="Cores"
					now={nowCores}
					target={targetCores}
					progress={reductionProgress(baseCores, nowCores, targetCores)}
				/>
				<Stat
					icon={<Layers className="size-4" />}
					label="Min set size"
					value={fmtInt(state.relay.minValidatorSetSize)}
					sub={`ah_client: ${state.relay.ahClientMode}`}
				/>
				<Stat
					icon={<Hourglass className="size-4" />}
					label="Next era boundary"
					value={countdown ? countdown.label : "—"}
					mono
					sub={
						currentStep
							? `→ ${fmtInt(currentStep.targets.validators)} val / ${fmtInt(
									currentStep.targets.cores,
								)} cores`
							: "membership changes at era edge"
					}
					pulse={!!countdown && !countdown.elapsed}
				/>
			</CardContent>
		</Card>
	);
}

function Metric({
	icon,
	label,
	now,
	target,
	progress,
}: {
	icon: React.ReactNode;
	label: string;
	now: number;
	target: number;
	progress: number;
}) {
	return (
		<div className="grid gap-2">
			<div className="text-muted-foreground flex items-center gap-1.5 text-sm">
				{icon}
				{label}
			</div>
			<div className="flex items-baseline gap-2">
				<span className="font-mono text-3xl font-semibold tabular-nums">
					{fmtInt(now)}
				</span>
				<span className="text-muted-foreground text-sm">
					→ {fmtInt(target)}
				</span>
			</div>
			<Progress value={progress * 100} indicatorClassName="bg-chart-2" />
			<div className="text-muted-foreground text-xs tabular-nums">
				{(progress * 100).toFixed(0)}% of planned reduction
			</div>
		</div>
	);
}

function Stat({
	icon,
	label,
	value,
	sub,
	mono,
	pulse,
}: {
	icon: React.ReactNode;
	label: string;
	value: string;
	sub?: string;
	mono?: boolean;
	pulse?: boolean;
}) {
	return (
		<div className="grid content-start gap-2">
			<div className="text-muted-foreground flex items-center gap-1.5 text-sm">
				{icon}
				{label}
			</div>
			<div className="flex items-center gap-2">
				<span
					className={`text-3xl font-semibold tabular-nums ${mono ? "font-mono" : ""}`}
				>
					{value}
				</span>
				{pulse ? (
					<span className="bg-chart-1 size-2 animate-pulse rounded-full" />
				) : null}
			</div>
			{sub ? <div className="text-muted-foreground text-xs">{sub}</div> : null}
		</div>
	);
}
