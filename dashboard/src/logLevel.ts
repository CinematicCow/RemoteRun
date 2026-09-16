import type { LogLine } from "./types";

export type LogLevel = "debug" | "info" | "warn" | "error";
export type LevelFilterValue = "all" | LogLevel;

export const LEVELS = ["all", "debug", "info", "warn", "error"] as const;

const LEVEL_RE = /\[(TRACE|DEBUG|INFO|WARN(?:ING)?|ERROR|ERR|FATAL)\]/i;

function levelOf(line: string): LogLevel | null {
  const m = LEVEL_RE.exec(line);
  if (!m) return null;
  const t = m[1].toUpperCase();
  if (t === "WARN" || t === "WARNING") return "warn";
  if (t === "ERROR" || t === "ERR" || t === "FATAL") return "error";
  if (t === "INFO") return "info";
  return "debug"; // DEBUG, TRACE
}

/** Level for a line: explicit token wins; bare stderr output counts as error
 *  so the tag, the counts and the level filter always agree. */
export function classify(l: LogLine): LogLevel | null {
  return levelOf(l.line) ?? (l.stream === "stderr" ? "error" : null);
}

/** Three-char gutter tag per level. The tag carries the level color so the
 *  message itself can stay neutral — a wall of red text is unreadable. */
export const LEVEL_TAG: Record<LogLevel, { label: string; tone: string }> = {
  debug: { label: "dbg", tone: "text-kumo-subtle" },
  info: { label: "inf", tone: "text-kumo-info" },
  warn: { label: "wrn", tone: "text-kumo-warning" },
  error: { label: "err", tone: "text-kumo-danger" },
};

/** Leading ISO-ish timestamp in the child's own output. Shown dimmed via the
 *  gutter instead of repeated at full strength inside the message. */
const TS_PREFIX =
  /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?\s+/;
/** Leading level token, e.g. `[ERROR] ` or `WARN: ` — replaced by the tag. */
const LEVEL_PREFIX = /^\[?(TRACE|DEBUG|INFO|WARN(?:ING)?|ERROR|ERR|FATAL)\]?:?\s+/i;

/** Message with redundant leading timestamp/level tokens folded into the
 *  gutter columns. Copy still uses the raw line — nothing is lost. */
export function displayText(raw: string): string {
  return raw.replace(TS_PREFIX, "").replace(LEVEL_PREFIX, "");
}

const timeFmt = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
});

export function formatTs(ts: string): string {
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? ts : timeFmt.format(d);
}
