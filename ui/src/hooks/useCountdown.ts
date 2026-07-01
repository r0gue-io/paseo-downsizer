"use client";

import { useEffect, useState } from "react";

export interface Countdown {
	totalMs: number;
	hours: number;
	minutes: number;
	seconds: number;
	/** true once the target time has passed. */
	elapsed: boolean;
	/** "HH:MM:SS" (or "MM:SS" under an hour). */
	label: string;
}

function compute(targetMs: number): Countdown {
	const now = Date.now();
	const totalMs = Math.max(0, targetMs - now);
	const totalSec = Math.floor(totalMs / 1000);
	const hours = Math.floor(totalSec / 3600);
	const minutes = Math.floor((totalSec % 3600) / 60);
	const seconds = totalSec % 60;
	const pad = (n: number) => String(n).padStart(2, "0");
	const label =
		hours > 0
			? `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`
			: `${pad(minutes)}:${pad(seconds)}`;
	return {
		totalMs,
		hours,
		minutes,
		seconds,
		elapsed: targetMs <= now,
		label,
	};
}

/** Live-ticking countdown (1s) to an ISO-8601 target timestamp. */
export function useCountdown(
	targetIso: string | null | undefined,
): Countdown | null {
	const targetMs = targetIso ? new Date(targetIso).getTime() : Number.NaN;
	const valid = Number.isFinite(targetMs);
	const [cd, setCd] = useState<Countdown | null>(() =>
		valid ? compute(targetMs) : null,
	);

	useEffect(() => {
		if (!valid) {
			setCd(null);
			return;
		}
		setCd(compute(targetMs));
		const id = setInterval(() => setCd(compute(targetMs)), 1000);
		return () => clearInterval(id);
	}, [targetMs, valid]);

	return cd;
}
