"use client";

import { Activity, RefreshCw } from "lucide-react";
import { CorePackingMap } from "@/components/dashboard/CorePackingMap";
import { DispatchLog } from "@/components/dashboard/DispatchLog";
import { FinalityHealth } from "@/components/dashboard/FinalityHealth";
import { HeadlineProgress } from "@/components/dashboard/HeadlineProgress";
import { ServiceBanner } from "@/components/dashboard/ServiceBanner";
import { StepTimeline } from "@/components/dashboard/StepTimeline";
import { ThemeToggle } from "@/components/dashboard/ThemeToggle";
import { usePoll } from "@/hooks/usePoll";
import { fetchHealth, fetchHistory, fetchPlan, fetchState } from "@/lib/api";
import { SERVICE_URL, USE_PAPI } from "@/lib/config";
import { fmtRelative } from "@/lib/format";
import { cn } from "@/lib/utils";

export function Dashboard() {
	const state = usePoll(fetchState);
	const plan = usePoll(fetchPlan);
	const history = usePoll(fetchHistory);
	const health = usePoll(fetchHealth);

	// Service unreachable = the state poll is erroring and we have nothing cached.
	const unreachable = !!state.error && state.data == null;
	const lastUpdated = state.lastUpdated;

	return (
		<div className="min-h-screen">
			<header className="bg-background/80 sticky top-0 z-20 border-b backdrop-blur">
				<div className="mx-auto flex max-w-[1400px] items-center justify-between gap-4 px-4 py-3 md:px-6">
					<div className="flex items-center gap-3">
						<div className="bg-chart-2/10 text-chart-2 flex size-9 items-center justify-center rounded-lg">
							<Activity className="size-5" />
						</div>
						<div className="grid">
							<h1 className="text-lg leading-tight font-semibold">
								Paseo Downsizer
							</h1>
							<p className="text-muted-foreground text-xs">
								Live relay validator + core downsizing
							</p>
						</div>
					</div>
					<div className="flex items-center gap-3">
						<LiveIndicator ok={!unreachable} lastUpdated={lastUpdated} />
						<ThemeToggle />
					</div>
				</div>
			</header>

			<main className="mx-auto grid max-w-[1400px] gap-4 px-4 py-4 md:px-6 md:py-6">
				<ServiceBanner unreachable={unreachable} health={health.data} />

				<HeadlineProgress state={state.data} plan={plan.data} />

				<div className="grid gap-4 lg:grid-cols-2">
					<StepTimeline plan={plan.data} />
					<FinalityHealth
						state={state.data}
						health={health.data}
						maxFinalityLag={plan.data?.maxFinalityLagBlocks}
						minOnlineFraction={plan.data?.minOnlineFraction}
					/>
				</div>

				<CorePackingMap state={state.data} plan={plan.data} />

				<DispatchLog history={history.data} />

				<footer className="text-muted-foreground flex flex-wrap items-center justify-between gap-2 pt-2 pb-6 text-xs">
					<span>
						Source of truth: on-chain state + planned tasks. Reads{" "}
						<span className="font-mono">{SERVICE_URL}</span>
						{USE_PAPI ? " + PAPI (live chain)" : ""}.
					</span>
					<span>Auto-refresh ~6s · countdowns tick live.</span>
				</footer>
			</main>
		</div>
	);
}

function LiveIndicator({
	ok,
	lastUpdated,
}: {
	ok: boolean;
	lastUpdated: number | null;
}) {
	return (
		<div className="text-muted-foreground flex items-center gap-2 text-xs">
			<span
				className={cn(
					"size-2 rounded-full",
					ok ? "bg-chart-2 animate-pulse" : "bg-destructive",
				)}
			/>
			{ok ? (
				<span className="hidden sm:inline">
					updated {fmtRelative(lastUpdated)}
				</span>
			) : (
				<span className="flex items-center gap-1">
					<RefreshCw className="size-3 animate-spin" />
					reconnecting
				</span>
			)}
		</div>
	);
}
