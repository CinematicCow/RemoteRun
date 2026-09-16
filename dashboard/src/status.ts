import type { ProcessStatus } from "./types";

export type StatusFilter = "all" | ProcessStatus;

/** Kumo Badge variant per status. */
export const statusBadge: Record<
  ProcessStatus,
  "success" | "neutral" | "error" | "warning"
> = {
  running: "success",
  stopped: "neutral",
  crashed: "error",
  backoff: "warning",
};

/* Static class literals so Tailwind generates them. */
export const statusDot: Record<ProcessStatus, string> = {
  running: "bg-kumo-success",
  backoff: "bg-kumo-warning",
  crashed: "bg-kumo-danger",
  stopped: "bg-kumo-fill",
};

export const statusLabel: Record<ProcessStatus, string> = {
  running: "Running",
  backoff: "Backoff",
  crashed: "Crashed",
  stopped: "Stopped",
};

/** Order used by the stat cards. */
export const STATUS_ORDER: ProcessStatus[] = [
  "running",
  "backoff",
  "crashed",
  "stopped",
];
