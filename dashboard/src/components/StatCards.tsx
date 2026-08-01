import { LayerCard, Text } from "@cloudflare/kumo";
import type { ProcessStatus } from "../types";

export type StatusFilter = "all" | ProcessStatus;

interface StatDef {
  status: ProcessStatus;
  label: string;
  dot: string;
}

/* Color lives in the status dot only — counts stay neutral so the cards
   read as data, not as four competing alerts. */
const STATS: StatDef[] = [
  { status: "running", label: "Running", dot: "bg-kumo-success" },
  { status: "backoff", label: "Backoff", dot: "bg-kumo-warning" },
  { status: "crashed", label: "Crashed", dot: "bg-kumo-danger" },
  { status: "stopped", label: "Stopped", dot: "bg-kumo-fill" },
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
    <div className="grid grid-cols-2 gap-2 sm:gap-3 lg:grid-cols-4">
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
              className={`h-full px-3 py-2.5 hover:bg-kumo-tint sm:px-5 sm:py-4 ${
                selected ? "ring-2 ring-kumo-brand" : ""
              }`}
            >
              {/* Mobile: one quiet line per chip. Desktop: label over count. */}
              <div className="flex items-center gap-2 sm:grid sm:gap-1">
                <div className="flex min-w-0 items-center gap-2">
                  <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${s.dot}`} />
                  <Text size="sm" variant="secondary" truncate>
                    {s.label}
                  </Text>
                </div>
                <span
                  className={`ml-auto text-sm font-semibold tabular-nums sm:ml-0 sm:text-2xl ${
                    count > 0 ? "text-kumo-default" : "text-kumo-subtle"
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
