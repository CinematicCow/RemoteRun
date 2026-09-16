import type { ReactNode, RefObject } from "react";
import { Button, ClipboardText } from "@cloudflare/kumo";
import { TerminalWindowIcon } from "@phosphor-icons/react";
import { formatAgo, formatUptime } from "../api";
import { useNow } from "../useNow";
import type { ProcessInfo } from "../types";

function displayUptime(p: ProcessInfo, fetchedAt: number, now: number): string {
  if (p.uptime_secs == null) return "—";
  const extra =
    p.status === "running"
      ? Math.max(0, Math.floor((now - fetchedAt) / 1000))
      : 0;
  return formatUptime(p.uptime_secs + extra);
}

/** The only piece of the page that re-renders every second. Keeping the
 *  clock in this leaf means uptime ticks without re-rendering the table. */
export function LiveUptime({
  p,
  fetchedAtRef,
}: {
  p: ProcessInfo;
  fetchedAtRef: RefObject<number>;
}) {
  const now = useNow(1000);
  return <>{displayUptime(p, fetchedAtRef.current, now)}</>;
}

function Meta({
  label,
  value,
  mono,
}: {
  label: string;
  value: ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="grid gap-1">
      <span className="text-xs text-kumo-subtle">{label}</span>
      <span
        className={`text-sm text-kumo-default ${mono ? "font-mono break-all" : "tabular-nums"}`}
      >
        {value}
      </span>
    </div>
  );
}

/** Expanded detail block, shared by the desktop table and mobile cards. */
export function ProcessDetails({
  p,
  fetchedAtRef,
  onLogs,
}: {
  p: ProcessInfo;
  fetchedAtRef: RefObject<number>;
  onLogs: (name: string) => void;
}) {
  return (
    <div className="grid min-w-0 gap-4">
      <div className="grid min-w-0 gap-1.5">
        <span className="text-xs text-kumo-subtle">Command</span>
        <ClipboardText text={p.command} className="font-mono text-sm" />
      </div>
      <div className="flex flex-wrap gap-x-10 gap-y-3">
        <Meta label="Pid" value={p.pid != null ? String(p.pid) : "—"} />
        <Meta
          label="Uptime"
          value={<LiveUptime p={p} fetchedAtRef={fetchedAtRef} />}
        />
        <Meta label="Restarts" value={String(p.restarts)} />
        <Meta
          label="Last crash"
          value={p.last_crash_at != null ? formatAgo(p.last_crash_at) : "—"}
        />
        <Meta
          label="Last exit code"
          value={p.last_exit_code != null ? String(p.last_exit_code) : "—"}
        />
        <Meta label="Working directory" value={p.cwd} mono />
        <Meta label="Created" value={formatAgo(p.created_at)} />
      </div>
      <div>
        <Button
          variant="secondary"
          size="sm"
          icon={<TerminalWindowIcon size={16} />}
          onClick={() => onLogs(p.name)}
        >
          View logs
        </Button>
      </div>
    </div>
  );
}
