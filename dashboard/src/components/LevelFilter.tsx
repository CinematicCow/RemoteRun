import { LEVELS, type LevelFilterValue } from "../logLevel";

const SEGMENT =
  "flex h-7 items-center rounded px-2.5 text-sm capitalize transition-transform active:scale-[0.96] focus-visible:ring-2 focus-visible:ring-kumo-brand focus-visible:outline-none";

export function LevelFilter({
  level,
  counts,
  onChange,
}: {
  level: LevelFilterValue;
  counts: Record<LevelFilterValue, number>;
  onChange: (level: LevelFilterValue) => void;
}) {
  return (
    <div
      role="group"
      aria-label="Filter by level"
      className="flex shrink-0 items-center rounded-lg bg-kumo-recessed p-1 ring ring-kumo-hairline"
    >
      {LEVELS.map((l) => {
        const active = level === l;
        const count = counts[l];
        return (
          <button
            key={l}
            type="button"
            aria-pressed={active}
            onClick={() => onChange(l)}
            className={`${SEGMENT} ${
              active
                ? "bg-kumo-base font-medium text-kumo-default shadow-sm"
                : "text-kumo-subtle hover:text-kumo-default"
            }`}
          >
            {l}
            {l !== "all" && count > 0 && (
              <span
                className={`ml-1.5 text-xs tabular-nums ${
                  active ? "text-kumo-subtle" : "text-kumo-subtle/60"
                }`}
              >
                {count}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
