import { useEffect, useMemo, useRef, useState } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import {
  Badge,
  Button,
  Dialog,
  InputGroup,
  Tooltip,
} from "@cloudflare/kumo";
import {
  ArrowDownIcon,
  BroomIcon,
  MagnifyingGlassIcon,
  PauseIcon,
  PlayIcon,
  XIcon,
} from "@phosphor-icons/react";
import { api } from "../api";
import type { LogLine } from "../types";

const MAX_LINES = 1000;

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

export function LogViewer({ name }: { name: string }) {
  const [lines, setLines] = useState<LogLine[]>([]);
  const [live, setLive] = useState(false);
  const [paused, setPaused] = useState(false);
  const [filter, setFilter] = useState("");
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
    if (!q) return lines;
    return lines.filter((l) => l.line.toLowerCase().includes(q));
  }, [lines, filter]);

  const jumpToLatest = () => {
    virtuosoRef.current?.scrollToIndex({
      index: visible.length - 1,
      align: "end",
    });
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

  return (
    <>
      <div className="mb-4 flex items-start justify-between gap-4">
        <div className="flex items-center gap-2">
          <Dialog.Title className="text-lg font-semibold">
            Logs — {name}
          </Dialog.Title>
          {paused ? (
            <Badge variant="warning" appearance="dot">
              paused
            </Badge>
          ) : (
            <Badge variant={live ? "success" : "neutral"} appearance="dot">
              {live ? "live" : "connecting"}
            </Badge>
          )}
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs tabular-nums text-kumo-subtle">
            {filter
              ? `${visible.length} / ${lines.length} lines`
              : `${lines.length} lines`}
          </span>
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
        </div>
      </div>

      <div className="mb-3 flex items-center gap-2">
        <InputGroup className="relative z-0 flex-1">
          <InputGroup.Addon>
            <MagnifyingGlassIcon size={16} />
          </InputGroup.Addon>
          <InputGroup.Input
            aria-label="Filter logs"
            placeholder="Filter output"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </InputGroup>
        <Tooltip
          content={paused ? "Resume stream" : "Pause stream"}
          render={
            <Button
              variant="secondary"
              shape="square"
              icon={paused ? <PlayIcon size={16} /> : <PauseIcon size={16} />}
              aria-label={paused ? "Resume stream" : "Pause stream"}
              onClick={togglePause}
            />
          }
        />
        <Tooltip
          content="Clear output"
          render={
            <Button
              variant="secondary"
              shape="square"
              icon={<BroomIcon size={16} />}
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

      <div className="relative">
        <div className="h-[52vh] overflow-hidden rounded-lg bg-kumo-recessed font-mono text-xs leading-relaxed ring ring-kumo-hairline">
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
                <div className="p-3 font-sans text-sm text-kumo-subtle">
                  {lines.length === 0
                    ? "No output yet…"
                    : "No lines match the filter."}
                </div>
              ),
            }}
            itemContent={(_i, l) => (
              <div className="whitespace-pre-wrap break-all px-3">
                <span className="mr-2 text-kumo-subtle">{formatTs(l.ts)}</span>
                <span
                  className={
                    l.stream === "stderr"
                      ? "text-kumo-danger"
                      : "text-kumo-default"
                  }
                >
                  {l.line}
                </span>
              </div>
            )}
          />
        </div>

        {!pinned && (
          <div className="absolute bottom-3 left-1/2 -translate-x-1/2">
            <Button
              variant="secondary"
              size="sm"
              icon={<ArrowDownIcon size={14} />}
              onClick={jumpToLatest}
              className="shadow-md"
            >
              {unseen > 0 ? `${unseen} new` : "Jump to latest"}
            </Button>
          </div>
        )}
      </div>
    </>
  );
}
