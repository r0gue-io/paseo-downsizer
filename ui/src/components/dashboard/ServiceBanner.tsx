"use client";

import { AlertTriangle, PauseCircle, WifiOff } from "lucide-react";
import { SERVICE_URL } from "@/lib/config";
import type { Health } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
	unreachable: boolean;
	health: Health | null;
}

/**
 * Banner shown when the service is unreachable, or when the dispatcher is
 * paused / in an error state. Renders nothing when everything is healthy.
 */
export function ServiceBanner({ unreachable, health }: Props) {
	if (unreachable) {
		return (
			<Bar tone="destructive" icon={<WifiOff className="size-4" />}>
				Service unreachable at <span className="font-mono">{SERVICE_URL}</span>.
				Showing last known data. Retrying every few seconds…
			</Bar>
		);
	}

	if (health && health.status !== "ok") {
		const paused = health.status === "paused";
		return (
			<Bar
				tone={paused ? "warning" : "destructive"}
				icon={
					paused ? (
						<PauseCircle className="size-4" />
					) : (
						<AlertTriangle className="size-4" />
					)
				}
			>
				Dispatcher {health.status}
				{health.reasons.length > 0 ? `: ${health.reasons.join("; ")}` : ""}
				{health.lastError ? ` — ${health.lastError}` : ""}
			</Bar>
		);
	}

	return null;
}

function Bar({
	tone,
	icon,
	children,
}: {
	tone: "destructive" | "warning";
	icon: React.ReactNode;
	children: React.ReactNode;
}) {
	return (
		<div
			className={cn(
				"flex items-center gap-2 rounded-lg border px-4 py-2.5 text-sm",
				tone === "destructive"
					? "border-destructive/40 bg-destructive/10 text-destructive"
					: "border-chart-1/40 bg-chart-1/10 text-foreground",
			)}
		>
			{icon}
			<span>{children}</span>
		</div>
	);
}
