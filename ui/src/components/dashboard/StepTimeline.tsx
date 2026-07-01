"use client";

import {
	CheckCircle2,
	CircleDashed,
	CircleDot,
	OctagonAlert,
	PauseCircle,
	PowerOff,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useCountdown } from "@/hooks/useCountdown";
import { fmtInt, fmtTime, stepStatusVariant } from "@/lib/format";
import type { Plan, PlanStep, StepStatus } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
	plan: Plan | null;
}

const ICON: Record<StepStatus, React.ComponentType<{ className?: string }>> = {
	done: CheckCircle2,
	active: CircleDot,
	pending: CircleDashed,
	paused: PauseCircle,
	failed: OctagonAlert,
	shutdown: PowerOff,
};

const DOT_COLOR: Record<StepStatus, string> = {
	done: "text-chart-2",
	active: "text-chart-1",
	pending: "text-muted-foreground",
	paused: "text-muted-foreground",
	failed: "text-destructive",
	shutdown: "text-destructive",
};

export function StepTimeline({ plan }: Props) {
	return (
		<Card className="h-full">
			<CardHeader>
				<CardTitle>Step timeline</CardTitle>
				<CardDescription>
					One step per era. Membership lands at each era boundary.
				</CardDescription>
			</CardHeader>
			<CardContent>
				{!plan ? (
					<div className="grid gap-4">
						{[0, 1, 2].map((i) => (
							<Skeleton key={i} className="h-16 w-full" />
						))}
					</div>
				) : (
					<ol className="relative">
						{plan.steps.map((step, idx) => (
							<StepRow
								key={step.id}
								step={step}
								isLast={idx === plan.steps.length - 1}
							/>
						))}
					</ol>
				)}
			</CardContent>
		</Card>
	);
}

function StepRow({ step, isLast }: { step: PlanStep; isLast: boolean }) {
	const Icon = step.shutdown ? PowerOff : (ICON[step.status] ?? CircleDashed);
	const dotColor = step.shutdown ? "text-destructive" : DOT_COLOR[step.status];
	const countdown = useCountdown(
		step.status === "active" ||
			step.status === "pending" ||
			step.status === "shutdown"
			? step.scheduledEta
			: null,
	);

	return (
		<li className="relative flex gap-3 pb-6 last:pb-0">
			{/* connector line */}
			{!isLast ? (
				<span
					className="bg-border absolute top-6 left-[11px] h-full w-px"
					aria-hidden
				/>
			) : null}
			<div className="relative z-10 mt-0.5">
				<Icon className={cn("size-6", dotColor)} />
			</div>
			<div className="grid flex-1 gap-1">
				<div className="flex flex-wrap items-center gap-2">
					<span className="font-medium">
						{step.shutdown ? "Network shutdown" : `Step ${step.id}`}
					</span>
					<Badge variant={stepStatusVariant(step.status)}>{step.status}</Badge>
					<span className="text-muted-foreground text-xs">
						era offset +{step.eraOffset}
					</span>
				</div>
				{step.shutdown ? (
					<div className="text-sm">
						Terminal — validator operators stop nodes; the chain halts. No
						on-chain reduction.
					</div>
				) : (
					<div className="text-sm">
						<span className="font-mono tabular-nums">
							{fmtInt(step.targets.validators)}
						</span>{" "}
						validators ·{" "}
						<span className="font-mono tabular-nums">
							{fmtInt(step.targets.cores)}
						</span>{" "}
						cores ·{" "}
						<span className="text-muted-foreground">
							min {fmtInt(step.targets.minSetSize)}
						</span>
					</div>
				)}
				{!step.shutdown && step.observed ? (
					<div className="text-muted-foreground text-xs">
						observed: {fmtInt(step.observed.validators)} val /{" "}
						{fmtInt(step.observed.cores)} cores
					</div>
				) : null}
				<div className="text-muted-foreground flex flex-wrap gap-x-4 gap-y-0.5 text-xs">
					{step.appliedAt ? (
						<span>applied {fmtTime(step.appliedAt)}</span>
					) : (
						<span>ETA {fmtTime(step.scheduledEta)}</span>
					)}
					{countdown && !countdown.elapsed ? (
						<span className="text-foreground font-mono tabular-nums">
							in {countdown.label}
						</span>
					) : null}
				</div>
				{step.note ? (
					<p className="text-muted-foreground text-xs italic">{step.note}</p>
				) : null}
			</div>
		</li>
	);
}
