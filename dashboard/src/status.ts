import type { ProcessStatus } from "./types";

export type StatusFilter = "all" | ProcessStatus;

interface StatusMeta {
  label: string;
  /** Tailwind dot colour — a static literal so Tailwind emits it. */
  dot: string;
  /** Kumo Badge variant. */
  badge: "success" | "neutral" | "error" | "warning";
}

/** Single source of truth for how each status is labelled and coloured. */
export const STATUS_META: Record<ProcessStatus, StatusMeta> = {
  running: { label: "Running", dot: "bg-kumo-success", badge: "success" },
  backoff: { label: "Backoff", dot: "bg-kumo-warning", badge: "warning" },
  crashed: { label: "Crashed", dot: "bg-kumo-danger", badge: "error" },
  stopped: { label: "Stopped", dot: "bg-kumo-fill", badge: "neutral" },
};

/** Display order for the stat cards. */
export const STATUS_ORDER: ProcessStatus[] = [
  "running",
  "backoff",
  "crashed",
  "stopped",
];
