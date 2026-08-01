import { LayerCard, Text } from "@cloudflare/kumo";
import type { ProcessStatus } from "../types";

export type StatusFilter = "all" | ProcessStatus;

interface StatDef {
  status: ProcessStatus;
  label: string;
  dot: string;
  /** Number color when count > 0 — semantic focus cue. */
  activeText: string;
  /** Pulse the dot when the count is non-zero (needs attention). */
  alert?: boolean;
}

const STATS: StatDef[] = [
  {
    status: "running",
    label: "Running",
    dot: "bg-kumo-success",
    activeText: "text-kumo-success",
  },
  {
    status: "backoff",
    label: "Backoff",
    dot: "bg-kumo-warning",
    activeText: "text-kumo-warning",
    alert: true,
  },
  {
    status: "crashed",
    label: "Crashed",
    dot: "bg-kumo-danger",
    activeText: "text-kumo-danger",
    alert: true,
  },
  {
    status: "stopped",
    label: "Stopped",
    dot: "bg-kumo-fill",
    activeText: "text-kumo-default",
  },
];

/* Static class literals so Tailwind generates them. */
const STAGGER = [
  "[animation-delay:0ms]",
  "[animation-delay:60ms]",
  "[animation-delay:120ms]",
  "[animation-delay:180ms]",
];

export function StatCards({
  counts,
  active,
  onSelect,
}: {
  counts: Record<ProcessStatus, number>;
  active: StatusFilter;
  onSelect: (status: ProcessStatus) => void;
}) {
  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      {STATS.map((s, i) => {
        const count = counts[s.status];
        const selected = active === s.status;
        return (
          <button
            key={s.status}
            type="button"
            aria-pressed={selected}
            onClick={() => onSelect(s.status)}
            className={`rr-rise rounded-xl text-left focus-visible:ring-2 focus-visible:ring-kumo-brand focus-visible:outline-none ${STAGGER[i]}`}
          >
            <LayerCard
              className={`h-full px-5 py-4 hover:bg-kumo-tint ${
                selected ? "ring-2 ring-kumo-brand" : ""
              }`}
            >
              <div className="grid gap-1">
                <div className="flex items-center gap-2">
                  <span
                    className={`h-1.5 w-1.5 rounded-full ${s.dot} ${
                      s.alert && count > 0 ? "rr-pulse" : ""
                    }`}
                  />
                  <Text size="sm" variant="secondary">
                    {s.label}
                  </Text>
                </div>
                <span
                  className={`text-2xl font-semibold tabular-nums ${
                    count > 0 ? s.activeText : "text-kumo-subtle"
                  }`}
                >
                  {count}
                </span>
              </div>
            </LayerCard>
          </button>
        );
      })}
    </div>
  );
}
