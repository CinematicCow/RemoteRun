import { useEffect, useMemo, useRef, useState } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { Badge, Button, Dialog, InputGroup, Tooltip } from "@cloudflare/kumo";
import { Drawer } from "vaul";
import {
  ArrowDownIcon,
  BroomIcon,
  MagnifyingGlassIcon,
  PauseIcon,
  PlayIcon,
  XIcon,
} from "@phosphor-icons/react";
import { api } from "../api";
import { CopyButton } from "./CopyButton";
import type { LogLine } from "../types";

const MAX_LINES = 1000;

type LogLevel = "debug" | "info" | "warn" | "error";

const LEVEL_RE = /\[(TRACE|DEBUG|INFO|WARN(?:ING)?|ERROR|ERR|FATAL)\]/i;

function logLevel(line: string): LogLevel | null {
  const m = LEVEL_RE.exec(line);
  if (!m) return null;
  const t = m[1].toUpperCase();
  if (t === "WARN" || t === "WARNING") return "warn";
  if (t === "ERROR" || t === "ERR" || t === "FATAL") return "error";
  if (t === "INFO") return "info";
  return "debug"; // DEBUG, TRACE
}

/** Level for a line: explicit token wins; bare stderr output counts as
 *  error so the tag, the counts and the level filter always agree. */
function classify(l: LogLine): LogLevel | null {
  return logLevel(l.line) ?? (l.stream === "stderr" ? "error" : null);
}

/** Three-char gutter tag per level. The tag carries the level color so the
 *  message itself can stay neutral — a wall of red text is unreadable. */
const LEVEL_TAG: Record<LogLevel, { label: string; tone: string }> = {
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
function displayText(raw: string): string {
  return raw.replace(TS_PREFIX, "").replace(LEVEL_PREFIX, "");
}

const timeFmt = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
});

function formatTs(ts: string): string {
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? ts : timeFmt.format(d);
}

export function LogViewer({
  name,
  chrome = "dialog",
  onClose,
}: {
  name: string;
  /** "dialog" renders inside a kumo Dialog; "drawer" inside the vaul sheet. */
  chrome?: "dialog" | "drawer";
  onClose?: () => void;
}) {
  const [lines, setLines] = useState<LogLine[]>([]);
  const [live, setLive] = useState(false);
  const [paused, setPaused] = useState(false);
  const [filter, setFilter] = useState("");
  const [level, setLevel] = useState<"all" | LogLevel>("all");
  const [pinned, setPinned] = useState(true);
  const [unseen, setUnseen] = useState(0);

  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const pausedRef = useRef(false);
  const pinnedRef = useRef(true);
  const pendingRef = useRef<LogLine[]>([]);

  pausedRef.current = paused;
  pinnedRef.current = pinned;

  /* History + live stream. */
  useEffect(() => {
    let source: EventSource | null = null;
    let cancelled = false;
    setLines([]);
    setLive(false);
    setUnseen(0);
    pendingRef.current = [];

    api
      .history(name)
      .then(({ lines }) => {
        if (cancelled) return;
        setLines(lines.slice(-MAX_LINES));
        source = new EventSource(api.streamUrl(name));
        source.onopen = () => setLive(true);
        source.onerror = () => setLive(false);
        source.onmessage = (e) => {
          const line: LogLine = JSON.parse(e.data);
          if (pausedRef.current) {
            pendingRef.current.push(line);
            if (pendingRef.current.length > MAX_LINES) {
              pendingRef.current.splice(0, pendingRef.current.length - MAX_LINES);
            }
            return;
          }
          setLines((prev) => [...prev.slice(-(MAX_LINES - 1)), line]);
          if (!pinnedRef.current) setUnseen((n) => n + 1);
        };
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      source?.close();
    };
  }, [name]);

  const visible = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return lines.filter((l) => {
      if (q && !l.line.toLowerCase().includes(q)) return false;
      if (level === "all") return true;
      /* When filtering by level, unclassified lines drop out. */
      return classify(l) === level;
    });
  }, [lines, filter, level]);

  /* Per-level counts for the segmented control, respecting the text filter
     so the numbers always match what selecting a segment would show. */
  const counts = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const c = { all: 0, debug: 0, info: 0, warn: 0, error: 0 };
    for (const l of lines) {
      if (q && !l.line.toLowerCase().includes(q)) continue;
      c.all += 1;
      const lvl = classify(l);
      if (lvl) c[lvl] += 1;
    }
    return c;
  }, [lines, filter]);



  const jumpToLatest = () => {
    if (visible.length > 0) {
      virtuosoRef.current?.scrollToIndex({
        index: visible.length - 1,
        align: "end",
      });
    }
    setPinned(true);
    setUnseen(0);
  };

  const togglePause = () => {
    if (paused) {
      /* Flush buffered lines on resume. */
      const buffered = pendingRef.current;
      pendingRef.current = [];
      if (buffered.length > 0) {
        setLines((prev) => [...prev, ...buffered].slice(-MAX_LINES));
      }
    }
    setPaused(!paused);
  };

  const title = (
    <>
      Logs <span className="text-kumo-subtle">—</span> {name}
    </>
  );

  return (
    <div
      className={
        chrome === "drawer" ? "flex h-full min-h-0 flex-col" : undefined
      }
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2.5">
          {chrome === "dialog" ? (
            <Dialog.Title className="truncate text-base font-semibold">
              {title}
            </Dialog.Title>
          ) : (
            <Drawer.Title className="truncate text-base font-semibold">
              {title}
            </Drawer.Title>
          )}
          <span className="shrink-0">
            {paused ? (
              <Badge variant="warning" appearance="dot">
                paused
              </Badge>
            ) : (
              <Badge variant={live ? "success" : "neutral"} appearance="dot">
                {live ? "live" : "connecting"}
              </Badge>
            )}
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-2.5">
          <span className="text-xs tabular-nums text-kumo-subtle">
            {filter || level !== "all"
              ? `${visible.length} / ${lines.length} lines`
              : `${lines.length} lines`}
          </span>
          {chrome === "dialog" ? (
            <Dialog.Close
              render={(props) => (
                <Button
                  {...props}
                  variant="ghost"
                  shape="square"
                  size="sm"
                  icon={<XIcon size={16} />}
                  aria-label="Close"
                />
              )}
            />
          ) : (
            <Button
              variant="ghost"
              shape="square"
              icon={<XIcon size={16} />}
              aria-label="Close"
              onClick={onClose}
            />
          )}
        </div>
      </div>

      {/* One toolbar row: compact level segments + search + stream controls.
          Segmented control keeps concentric radii (track 8px = segment 4px +
          padding 4px) but hugs its content instead of filling a row. */}
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div
          role="group"
          aria-label="Filter by level"
          className="flex shrink-0 items-center rounded-lg bg-kumo-recessed p-1 ring ring-kumo-hairline"
        >
          {(["all", "debug", "info", "warn", "error"] as const).map((l) => {
            const active = level === l;
            const count = l === "all" ? counts.all : counts[l];
            return (
              <button
                key={l}
                type="button"
                aria-pressed={active}
                onClick={() => setLevel(l)}
                className={`flex h-7 items-center rounded px-2.5 text-sm capitalize transition-transform active:scale-[0.96] focus-visible:ring-2 focus-visible:ring-kumo-brand focus-visible:outline-none ${
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

        <InputGroup className="relative z-0 min-w-[200px] flex-1">
        <InputGroup.Addon>
          <MagnifyingGlassIcon size={16} />
        </InputGroup.Addon>
        <InputGroup.Input
          aria-label="Filter logs"
          placeholder="Filter output"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <InputGroup.Addon align="end">
          <div className="flex items-center gap-0.5">
            <CopyButton
              size="xs"
              getText={() => visible.map((l) => `${l.ts} ${l.line}`).join("\n")}
              label="Copy logs"
            />
            <Tooltip
              content={paused ? "Resume stream" : "Pause stream"}
              render={
                <Button
                  variant="ghost"
                  shape="square"
                  size="xs"
                  icon={
                    paused ? <PlayIcon size={14} /> : <PauseIcon size={14} />
                  }
                  aria-label={paused ? "Resume stream" : "Pause stream"}
                  onClick={togglePause}
                />
              }
            />
            <Tooltip
              content="Clear output"
              render={
                <Button
                  variant="ghost"
                  shape="square"
                  size="xs"
                  icon={<BroomIcon size={14} />}
                  aria-label="Clear output"
                  onClick={() => {
                    setLines([]);
                    setUnseen(0);
                    pendingRef.current = [];
                  }}
                />
              }
            />
            </div>
          </InputGroup.Addon>
        </InputGroup>
      </div>

      <div
        className={
          chrome === "drawer"
            ? "relative flex min-h-0 flex-1 flex-col"
            : "relative"
        }
      >
        <div
          className={`overflow-hidden rounded-lg bg-kumo-recessed font-mono text-[13px] leading-[1.55] ring ring-kumo-hairline ${
            chrome === "drawer" ? "min-h-0 flex-1" : "h-[52vh]"
          }`}
        >
          <Virtuoso
            ref={virtuosoRef}
            style={{ height: "100%" }}
            className="rr-log-scroll"
            data={visible}
            followOutput="auto"
            atBottomStateChange={(atBottom) => {
              setPinned(atBottom);
              if (atBottom) setUnseen(0);
            }}
            components={{
              /* Breathing room without padding on the scroller itself. */
              Header: () => <div className="h-3" />,
              Footer: () => <div className="h-3" />,
              EmptyPlaceholder: () => (
                <div className="flex h-full items-center justify-center p-6 font-sans text-sm text-kumo-subtle">
                  {lines.length === 0
                    ? "No output yet…"
                    : "No lines match the filter."}
                </div>
              ),
            }}
            itemContent={(_i, l) => {
              const lvl = classify(l);
              return (
                <div
                  className={`flex gap-2.5 px-3 py-px ${
                    lvl === "error"
                      ? "bg-kumo-danger-tint/25"
                      : "hover:bg-kumo-tint"
                  }`}
                >
                  <span className="shrink-0 select-none tabular-nums text-kumo-subtle/80">
                    {formatTs(l.ts)}
                  </span>
                  <span
                    className={`w-[3ch] shrink-0 select-none font-medium ${
                      lvl ? LEVEL_TAG[lvl].tone : ""
                    }`}
                  >
                    {lvl ? LEVEL_TAG[lvl].label : ""}
                  </span>
                  {/* break-words, not break-all: only split a token when it
                      genuinely can't fit, so words stay intact. */}
                  <span className="min-w-0 flex-1 whitespace-pre-wrap break-words">
                    {displayText(l.line)}
                  </span>
                </div>
              );
            }}
          />
        </div>

        {!pinned && (
          <div className="rr-rise absolute bottom-3 left-1/2 -translate-x-1/2">
            <Button
              variant="secondary"
              size="sm"
              icon={<ArrowDownIcon size={14} />}
              onClick={jumpToLatest}
              className="rounded-full shadow-lg"
            >
              <span className="tabular-nums">
                {unseen > 0
                  ? `${unseen} new ${unseen === 1 ? "line" : "lines"}`
                  : "Jump to latest"}
              </span>
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
