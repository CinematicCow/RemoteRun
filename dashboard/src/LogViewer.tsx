import { useEffect, useRef, useState } from "react";
import { Badge, Button, Text } from "@cloudflare/kumo";
import { XIcon } from "@phosphor-icons/react";
import { api } from "./api";
import type { LogLine } from "./types";

const MAX_LINES = 1000;

export function LogViewer({
  name,
  onClose,
}: {
  name: string;
  onClose: () => void;
}) {
  const [lines, setLines] = useState<LogLine[]>([]);
  const [live, setLive] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let source: EventSource | null = null;
    let cancelled = false;
    setLines([]);
    setLive(false);

    api
      .history(name)
      .then(({ lines }) => {
        if (cancelled) return;
        setLines(lines);
        source = new EventSource(api.streamUrl(name));
        source.onopen = () => setLive(true);
        source.onerror = () => setLive(false);
        source.onmessage = (e) => {
          const line: LogLine = JSON.parse(e.data);
          setLines((prev) => [...prev.slice(-(MAX_LINES - 1)), line]);
        };
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      source?.close();
    };
  }, [name]);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [lines]);

  return (
    <div className="log-viewer">
      <div className="log-viewer-header">
        <div className="flex items-center gap-2">
          <Text as="h2" variant="heading3">
            Logs — {name}
          </Text>
          <Badge
            variant={live ? "success" : "neutral"}
            appearance="dot"
          >
            {live ? "live" : "connecting"}
          </Badge>
        </div>
        <Button variant="ghost" size="xs" icon={XIcon} onClick={onClose}>
          Close
        </Button>
      </div>
      <div className="log-lines" ref={scrollRef}>
        {lines.length === 0 ? (
          <span className="log-line-ts">no output yet…</span>
        ) : (
          lines.map((l, i) => (
            <div key={i} className={`log-line ${l.stream}`}>
              <span className="log-line-ts">{l.ts}</span>
              <span className="log-line-text">{l.line}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
