"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { POLL_INTERVAL_MS } from "@/lib/config";

export interface PollResult<T> {
	data: T | null;
	error: Error | null;
	/** True until the first response (success or failure) arrives. */
	loading: boolean;
	/** Timestamp (ms) of the last successful fetch. */
	lastUpdated: number | null;
	refetch: () => void;
}

/**
 * Polls an async fetcher on a fixed interval. Keeps the last good `data` when a
 * later poll fails (so transient service-unreachable blips don't blank the UI),
 * while surfacing the `error` for a banner.
 */
export function usePoll<T>(
	fetcher: (signal: AbortSignal) => Promise<T>,
	intervalMs: number = POLL_INTERVAL_MS,
): PollResult<T> {
	const [data, setData] = useState<T | null>(null);
	const [error, setError] = useState<Error | null>(null);
	const [loading, setLoading] = useState(true);
	const [lastUpdated, setLastUpdated] = useState<number | null>(null);
	const [tick, setTick] = useState(0);

	// Keep the fetcher stable-ish via ref so callers can pass inline fns.
	const fetcherRef = useRef(fetcher);
	fetcherRef.current = fetcher;

	const refetch = useCallback(() => setTick((t) => t + 1), []);

	// `tick` is an intentional dependency: bumping it (via refetch) forces an
	// immediate re-poll. The fetcher itself is read through a ref, so it is
	// deliberately excluded.
	// biome-ignore lint/correctness/useExhaustiveDependencies: see above.
	useEffect(() => {
		let cancelled = false;
		const controller = new AbortController();

		const run = async () => {
			try {
				const result = await fetcherRef.current(controller.signal);
				if (cancelled) return;
				setData(result);
				setError(null);
				setLastUpdated(Date.now());
			} catch (err) {
				if (cancelled || controller.signal.aborted) return;
				setError(err as Error);
			} finally {
				if (!cancelled) setLoading(false);
			}
		};

		run();
		const id = setInterval(run, intervalMs);
		return () => {
			cancelled = true;
			controller.abort();
			clearInterval(id);
		};
	}, [intervalMs, tick]);

	return { data, error, loading, lastUpdated, refetch };
}
