import { LayerCard, Text } from "@cloudflare/kumo";
import { STATUS_META, STATUS_ORDER, type StatusFilter } from "../status";
import type { ProcessStatus } from "../types";

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
      {STATUS_ORDER.map((status, i) => {
        const count = counts[status];
        const selected = active === status;
        const meta = STATUS_META[status];
        return (
          <button
            key={status}
            type="button"
            aria-pressed={selected}
            onClick={() => onSelect(status)}
            className={`rr-rise rounded-xl text-left focus-visible:ring-2 focus-visible:ring-kumo-brand focus-visible:outline-none ${STAGGER[i]}`}
          >
            <LayerCard
              className={`h-full px-4 py-3.5 hover:bg-kumo-tint sm:px-5 sm:py-4 ${
                selected ? "ring-2 ring-kumo-brand" : ""
              }`}
            >
              {/* Mobile: one quiet line per chip. Desktop: label over count. */}
              <div className="flex items-center gap-2 sm:grid sm:gap-1">
                <div className="flex min-w-0 items-center gap-2">
                  <span
                    className={`h-1.5 w-1.5 shrink-0 rounded-full ${meta.dot}`}
                  />
                  <Text size="sm" variant="secondary" truncate>
                    {meta.label}
                  </Text>
                </div>
                <span
                  className={`ml-auto text-base font-semibold tabular-nums sm:ml-0 sm:text-2xl ${
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
