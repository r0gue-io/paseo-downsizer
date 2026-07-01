"use client";

import { Activity, ShieldAlert, ShieldCheck } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { fmtInt, fmtPct } from "@/lib/format";
import type { ChainState, Health } from "@/lib/types";

interface Props {
	state: ChainState | null;
	health: Health | null;
	/** Finality lag above which the scheduler pauses (plan health.max_finality_lag_blocks). */
	maxFinalityLag?: number;
	/** Online fraction below which the scheduler pauses (plan health.min_online_fraction). */
	minOnlineFraction?: number;
}

// Byzantine safety floor: finality needs > 2/3 of the set online.
const DANGER_FRACTION = 2 / 3;

export function FinalityHealth({
	state,
	health,
	maxFinalityLag = 10,
	minOnlineFraction = 0.8,
}: Props) {
	if (!state) {
		return (
			<Card className="h-full">
				<CardHeader>
					<CardTitle>Finality health</CardTitle>
				</CardHeader>
				<CardContent className="grid gap-4">
					<Skeleton className="mx-auto h-28 w-52" />
					<Skeleton className="h-16 w-full" />
				</CardContent>
			</Card>
		);
	}

	const { relay } = state;
	const online = relay.onlineFraction;
	const lagOk = relay.finalityLag <= maxFinalityLag;
	const onlineOk = online == null ? true : online >= minOnlineFraction;
	const healthy = lagOk && onlineOk && (health?.status ?? "ok") === "ok";

	return (
		<Card className="h-full">
			<CardHeader className="flex-row items-center justify-between gap-4">
				<div className="grid gap-1.5">
					<CardTitle>Finality health</CardTitle>
					<CardDescription>
						GRANDPA lag, AH client mode, liveness (finality-derived
						estimate — no imOnline pallet post-AHM).
					</CardDescription>
				</div>
				<Badge
					variant={healthy ? "secondary" : "destructive"}
					className="gap-1.5"
				>
					{healthy ? (
						<ShieldCheck className="size-3" />
					) : (
						<ShieldAlert className="size-3" />
					)}
					{healthy ? "healthy" : "attention"}
				</Badge>
			</CardHeader>
			<CardContent className="grid gap-5">
				<OnlineGauge
					fraction={online}
					threshold={minOnlineFraction}
					danger={DANGER_FRACTION}
				/>

				<Separator />

				<div className="grid grid-cols-2 gap-4 text-sm">
					<Field label="Best block">
						<span className="font-mono tabular-nums">
							#{fmtInt(relay.bestBlock)}
						</span>
					</Field>
					<Field label="Finalized">
						<span className="font-mono tabular-nums">
							#{fmtInt(relay.finalizedBlock)}
						</span>
					</Field>
					<Field label="Finality lag">
						<span
							className={`font-mono tabular-nums ${
								lagOk ? "" : "text-destructive"
							}`}
						>
							{fmtInt(relay.finalityLag)} blk
						</span>
						<span className="text-muted-foreground text-xs">
							{" "}
							/ max {maxFinalityLag}
						</span>
					</Field>
					<Field label="ah_client mode">
						<Badge variant="outline">{relay.ahClientMode}</Badge>
					</Field>
					<Field label="Dispatcher">
						<Badge
							variant={health?.dispatcher === "armed" ? "default" : "outline"}
						>
							{health?.dispatcher ?? "—"}
						</Badge>
					</Field>
					<Field label="Session">
						<span className="font-mono tabular-nums">
							{fmtInt(relay.sessionIndex)}
						</span>
					</Field>
				</div>
			</CardContent>
		</Card>
	);
}

function Field({
	label,
	children,
}: {
	label: string;
	children: React.ReactNode;
}) {
	return (
		<div className="grid gap-1">
			<span className="text-muted-foreground text-xs">{label}</span>
			<span className="flex items-center gap-1">{children}</span>
		</div>
	);
}

// Semicircular gauge (plain SVG) using the --chart-* CSS vars. Marks the 2/3
// Byzantine danger line and the configured pause threshold.
function OnlineGauge({
	fraction,
	threshold,
	danger,
}: {
	fraction: number | undefined;
	threshold: number;
	danger: number;
}) {
	const W = 220;
	const H = 124;
	const cx = W / 2;
	const cy = 112;
	const r = 92;

	const pt = (frac: number) => {
		const a = Math.PI - Math.max(0, Math.min(1, frac)) * Math.PI;
		return { x: cx + r * Math.cos(a), y: cy - r * Math.sin(a) };
	};
	const arc = (from: number, to: number) => {
		const s = pt(from);
		const e = pt(to);
		// The gauge spans at most 180°, so every arc is the minor arc
		// (large-arc = 0), drawn clockwise OVER THE TOP (sweep = 1) because frac
		// increases left → top → right. (Previously sweep=0 drew the bottom half.)
		return `M ${s.x} ${s.y} A ${r} ${r} 0 0 1 ${e.x} ${e.y}`;
	};
	const tick = (frac: number) => {
		const outer = pt(frac);
		const a = Math.PI - frac * Math.PI;
		const inner = {
			x: cx + (r - 14) * Math.cos(a),
			y: cy - (r - 14) * Math.sin(a),
		};
		return { outer, inner };
	};

	const hasValue = fraction != null && Number.isFinite(fraction);
	const value = hasValue ? Math.max(0, Math.min(1, fraction as number)) : 0;

	const valueColor =
		!hasValue || value < danger
			? "var(--destructive)"
			: value < threshold
				? "var(--chart-1)"
				: "var(--chart-2)";

	const dangerTick = tick(danger);
	const thresholdTick = tick(threshold);
	const needle = pt(value);

	return (
		<div className="flex flex-col items-center">
			<svg
				width={W}
				height={H}
				viewBox={`0 0 ${W} ${H}`}
				role="img"
				aria-label={`Online fraction ${hasValue ? fmtPct(value) : "unknown"}`}
			>
				{/* track */}
				<path
					d={arc(0, 1)}
					fill="none"
					stroke="var(--muted)"
					strokeWidth={16}
					strokeLinecap="round"
				/>
				{/* value */}
				{hasValue ? (
					<path
						d={arc(0, value)}
						fill="none"
						stroke={valueColor}
						strokeWidth={16}
						strokeLinecap="round"
					/>
				) : null}

				{/* 2/3 danger line */}
				<Tick t={dangerTick} color="var(--destructive)" />
				{/* pause threshold */}
				<Tick t={thresholdTick} color="var(--foreground)" />

				{/* needle dot */}
				{hasValue ? (
					<circle cx={needle.x} cy={needle.y} r={5} fill={valueColor} />
				) : null}

				<text
					x={cx}
					y={cy - 20}
					textAnchor="middle"
					className="fill-foreground"
					style={{ fontSize: 26, fontWeight: 600 }}
				>
					{hasValue ? fmtPct(value, 1) : "n/a"}
				</text>
				<text
					x={cx}
					y={cy - 2}
					textAnchor="middle"
					className="fill-muted-foreground"
					style={{ fontSize: 11 }}
				>
					liveness (est.)
				</text>
			</svg>
			<div className="text-muted-foreground flex items-center gap-4 text-xs">
				<Tooltip>
					<TooltipTrigger asChild>
						<span className="flex items-center gap-1">
							<span
								className="inline-block h-2 w-2 rounded-full"
								style={{ background: "var(--destructive)" }}
							/>
							2/3 floor ({fmtPct(danger, 1)})
						</span>
					</TooltipTrigger>
					<TooltipContent>
						Below this, GRANDPA cannot finalize (Byzantine safety limit).
					</TooltipContent>
				</Tooltip>
				<Tooltip>
					<TooltipTrigger asChild>
						<span className="flex items-center gap-1">
							<Activity className="size-3" />
							pause &lt; {fmtPct(threshold, 0)}
						</span>
					</TooltipTrigger>
					<TooltipContent>
						Scheduler auto-pauses if the online fraction drops below this.
					</TooltipContent>
				</Tooltip>
			</div>
		</div>
	);
}

function Tick({
	t,
	color,
}: {
	t: { outer: { x: number; y: number }; inner: { x: number; y: number } };
	color: string;
}) {
	return (
		<line
			x1={t.inner.x}
			y1={t.inner.y}
			x2={t.outer.x}
			y2={t.outer.y}
			stroke={color}
			strokeWidth={2.5}
		/>
	);
}
