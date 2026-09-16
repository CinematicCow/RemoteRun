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
import {
  classify,
  displayText,
  formatTs,
  LEVEL_TAG,
  type LevelFilterValue,
} from "../logLevel";
import { CopyButton } from "./CopyButton";
import { LevelFilter } from "./LevelFilter";
import type { LogLine } from "../types";

const MAX_LINES = 1000;

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
  const [level, setLevel] = useState<LevelFilterValue>("all");
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

    /* Chatty processes can emit dozens of lines between frames; buffer them
       and commit once per animation frame instead of one render per line. */
    let batch: LogLine[] = [];
    let frame: number | null = null;
    const flush = () => {
      frame = null;
      const commit = batch;
      batch = [];
      if (cancelled || commit.length === 0) return;
      setLines((prev) => [...prev, ...commit].slice(-MAX_LINES));
      if (!pinnedRef.current) setUnseen((n) => n + commit.length);
    };

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
          batch.push(line);
          if (batch.length > MAX_LINES) {
            batch.splice(0, batch.length - MAX_LINES);
          }
          frame ??= requestAnimationFrame(flush);
        };
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      if (frame != null) cancelAnimationFrame(frame);
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
    const c: Record<LevelFilterValue, number> = {
      all: 0,
      debug: 0,
      info: 0,
      warn: 0,
      error: 0,
    };
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

      {/* One toolbar row: compact level segments + search + stream controls. */}
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <LevelFilter level={level} counts={counts} onChange={setLevel} />

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
                getText={() =>
                  visible.map((l) => `${l.ts} ${l.line}`).join("\n")
                }
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
