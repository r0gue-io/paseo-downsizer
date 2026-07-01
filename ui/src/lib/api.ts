// Thin client for the Rust downsizer service JSON API (SPEC.md contract).

import { SERVICE_URL } from "@/lib/config";
import type {
	ChainState,
	ControlRequest,
	Health,
	History,
	Plan,
} from "@/lib/types";

export class ServiceError extends Error {
	constructor(
		message: string,
		readonly status?: number,
	) {
		super(message);
		this.name = "ServiceError";
	}
}

async function getJson<T>(path: string, signal?: AbortSignal): Promise<T> {
	let res: Response;
	try {
		res = await fetch(`${SERVICE_URL}${path}`, {
			signal,
			cache: "no-store",
			headers: { accept: "application/json" },
		});
	} catch (err) {
		throw new ServiceError(
			`service unreachable at ${SERVICE_URL}: ${(err as Error).message}`,
		);
	}
	if (!res.ok) {
		throw new ServiceError(
			`GET ${path} -> ${res.status} ${res.statusText}`,
			res.status,
		);
	}
	return (await res.json()) as T;
}

export const fetchState = (signal?: AbortSignal) =>
	getJson<ChainState>("/api/state", signal);

export const fetchPlan = (signal?: AbortSignal) =>
	getJson<Plan>("/api/plan", signal);

export const fetchHistory = (signal?: AbortSignal) =>
	getJson<History>("/api/history", signal);

export const fetchHealth = (signal?: AbortSignal) =>
	getJson<Health>("/api/health", signal);

/** POST /api/control — pause/resume the dispatcher (optional bearer token). */
export async function postControl(
	body: ControlRequest,
	token?: string,
): Promise<void> {
	const res = await fetch(`${SERVICE_URL}/api/control`, {
		method: "POST",
		headers: {
			"content-type": "application/json",
			...(token ? { authorization: `Bearer ${token}` } : {}),
		},
		body: JSON.stringify(body),
	});
	if (!res.ok) {
		throw new ServiceError(
			`POST /api/control -> ${res.status} ${res.statusText}`,
			res.status,
		);
	}
}
