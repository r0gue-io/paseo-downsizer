"use client";

import { ExternalLink } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { explorerTxUrl } from "@/lib/config";
import { fmtTime, historyStatusVariant, shortHash } from "@/lib/format";
import type { History } from "@/lib/types";

interface Props {
	history: History | null;
}

export function DispatchLog({ history }: Props) {
	const rows = history ? history.slice().reverse() : null;

	return (
		<Card>
			<CardHeader>
				<CardTitle>Dispatch log</CardTitle>
				<CardDescription>
					Newest first — dry-run, in-block, finalized, or error.
				</CardDescription>
			</CardHeader>
			<CardContent>
				{!rows ? (
					<div className="grid gap-2">
						{[0, 1, 2, 3].map((i) => (
							<Skeleton key={i} className="h-9 w-full" />
						))}
					</div>
				) : rows.length === 0 ? (
					<p className="text-muted-foreground text-sm">
						No dispatches yet. Calls appear here as the scheduler fires each
						step.
					</p>
				) : (
					<Table>
						<TableHeader>
							<TableRow>
								<TableHead>Time</TableHead>
								<TableHead>Step</TableHead>
								<TableHead>Chain</TableHead>
								<TableHead>Call</TableHead>
								<TableHead>Args</TableHead>
								<TableHead>Status</TableHead>
								<TableHead>Tx</TableHead>
							</TableRow>
						</TableHeader>
						<TableBody>
							{rows.map((h, i) => {
								const url = explorerTxUrl(h.txHash);
								return (
									<TableRow key={`${h.txHash || "notx"}-${h.at}-${i}`}>
										<TableCell className="text-muted-foreground whitespace-nowrap">
											{fmtTime(h.at)}
										</TableCell>
										<TableCell className="tabular-nums">{h.stepId}</TableCell>
										<TableCell>
											<Badge variant="outline">{h.chain}</Badge>
										</TableCell>
										<TableCell className="font-mono text-xs">
											{h.call}
										</TableCell>
										<TableCell className="text-muted-foreground max-w-[22ch] truncate text-xs">
											{h.argsSummary}
										</TableCell>
										<TableCell>
											<Badge variant={historyStatusVariant(h.status)}>
												{h.status}
											</Badge>
											{h.error ? (
												<span
													className="text-destructive ml-1 text-xs"
													title={h.error}
												>
													!
												</span>
											) : null}
										</TableCell>
										<TableCell className="font-mono text-xs">
											{h.txHash ? (
												url ? (
													<a
														href={url}
														target="_blank"
														rel="noreferrer"
														className="text-chart-2 inline-flex items-center gap-1 hover:underline"
													>
														{shortHash(h.txHash)}
														<ExternalLink className="size-3" />
													</a>
												) : (
													<span>{shortHash(h.txHash)}</span>
												)
											) : (
												<span className="text-muted-foreground">—</span>
											)}
										</TableCell>
									</TableRow>
								);
							})}
						</TableBody>
					</Table>
				)}
			</CardContent>
		</Card>
	);
}
