import { Fragment, useState } from "react";
import {
  Badge,
  Button,
  ClipboardText,
  DropdownMenu,
  Table,
} from "@cloudflare/kumo";
import {
  ArrowClockwiseIcon,
  CaretRightIcon,
  DotsThreeVerticalIcon,
  StopIcon,
  TerminalWindowIcon,
  TrashIcon,
} from "@phosphor-icons/react";
import { formatAgo, formatUptime } from "../api";
import type { ProcessInfo, ProcessStatus } from "../types";

const statusBadge: Record<
  ProcessStatus,
  "success" | "neutral" | "error" | "warning"
> = {
  running: "success",
  stopped: "neutral",
  crashed: "error",
  backoff: "warning",
};

export interface ProcessActions {
  onLogs: (name: string) => void;
  onRestart: (name: string) => void;
  onStop: (name: string) => void;
  onRemove: (p: ProcessInfo) => void;
}

interface Props extends ProcessActions {
  processes: ProcessInfo[];
  /** When the current process snapshot was fetched (for live uptime ticks). */
  fetchedAt: number;
  now: number;
}

function displayUptime(p: ProcessInfo, fetchedAt: number, now: number): string {
  if (p.uptime_secs == null) return "—";
  const extra =
    p.status === "running"
      ? Math.max(0, Math.floor((now - fetchedAt) / 1000))
      : 0;
  return formatUptime(p.uptime_secs + extra);
}

/** Left edge stripe token for unhealthy processes. */
function stripe(p: ProcessInfo): string {
  if (p.status === "crashed") {
    return "shadow-[inset_2px_0_0_var(--color-kumo-danger)]";
  }
  if (p.status === "backoff") {
    return "shadow-[inset_2px_0_0_var(--color-kumo-warning)]";
  }
  return "";
}

function Meta({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
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

function ActionsMenu({
  p,
  size = "sm",
  onLogs,
  onRestart,
  onStop,
  onRemove,
}: ProcessActions & { p: ProcessInfo; size?: "sm" | "base" }) {
  const alive = p.status === "running" || p.status === "backoff";
  return (
    <DropdownMenu>
      <DropdownMenu.Trigger
        render={
          <Button
            variant="ghost"
            shape="square"
            size={size}
            icon={<DotsThreeVerticalIcon size={16} />}
            aria-label={`Actions for ${p.name}`}
          />
        }
      />
      <DropdownMenu.Content>
        <DropdownMenu.Item
          icon={TerminalWindowIcon}
          onClick={() => onLogs(p.name)}
        >
          View logs
        </DropdownMenu.Item>
        <DropdownMenu.Item
          icon={ArrowClockwiseIcon}
          onClick={() => onRestart(p.name)}
        >
          Restart
        </DropdownMenu.Item>
        {alive ? (
          <DropdownMenu.Item icon={StopIcon} onClick={() => onStop(p.name)}>
            Stop
          </DropdownMenu.Item>
        ) : (
          <DropdownMenu.Item
            variant="danger"
            icon={TrashIcon}
            onClick={() => onRemove(p)}
          >
            Remove
          </DropdownMenu.Item>
        )}
      </DropdownMenu.Content>
    </DropdownMenu>
  );
}

function Caret({ open }: { open: boolean }) {
  return (
    <span className="flex h-lh shrink-0 items-center text-kumo-subtle">
      <CaretRightIcon
        size={12}
        className={`transition-transform duration-150 ease-out ${
          open ? "rotate-90" : ""
        }`}
      />
    </span>
  );
}

/** Expanded detail block, shared by the desktop table and mobile cards. */
function ProcessDetails({
  p,
  fetchedAt,
  now,
  onLogs,
}: {
  p: ProcessInfo;
  fetchedAt: number;
  now: number;
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
        <Meta label="Uptime" value={displayUptime(p, fetchedAt, now)} />
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

/** Compact secondary line for mobile cards, with semantic tones. */
function CardMeta({
  p,
  fetchedAt,
  now,
}: {
  p: ProcessInfo;
  fetchedAt: number;
  now: number;
}) {
  const crashFresh =
    p.last_crash_at != null && now / 1000 - p.last_crash_at < 300;
  const parts: { text: string; tone: string }[] = [];
  if (p.uptime_secs != null) {
    parts.push({
      text: `up ${displayUptime(p, fetchedAt, now)}`,
      tone: "text-kumo-subtle",
    });
  }
  if (p.restarts > 0) {
    parts.push({
      text: `${p.restarts} restarts`,
      tone: "text-kumo-warning",
    });
  }
  if (p.last_crash_at != null) {
    parts.push({
      text: `crashed ${formatAgo(p.last_crash_at)}`,
      tone: crashFresh ? "text-kumo-danger" : "text-kumo-subtle",
    });
  }
  if (parts.length === 0) {
    return (
      <span className="text-xs text-kumo-subtle">
        created {formatAgo(p.created_at)}
      </span>
    );
  }
  return (
    <span className="text-xs tabular-nums">
      {parts.map((part, i) => (
        <span key={i}>
          {i > 0 && <span className="text-kumo-subtle"> · </span>}
          <span className={part.tone}>{part.text}</span>
        </span>
      ))}
    </span>
  );
}

export function ProcessTable({
  processes,
  fetchedAt,
  now,
  onLogs,
  onRestart,
  onStop,
  onRemove,
}: Props) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const toggle = (name: string) =>
    setExpanded((cur) => (cur === name ? null : name));

  const actions = { onLogs, onRestart, onStop, onRemove };

  return (
    <>
      {/* Mobile: card list. */}
      <div className="flex flex-col gap-2 md:hidden">
        {processes.map((p) => {
          const open = expanded === p.name;
          return (
            <div
              key={p.name}
              className={`rounded-xl bg-kumo-base ring ring-kumo-line ${stripe(p)}`}
            >
              <button
                type="button"
                onClick={() => toggle(p.name)}
                aria-expanded={open}
                className="flex w-full items-center justify-between gap-2 px-3 pt-2.5 pb-1 text-left"
              >
                <span className="flex min-w-0 items-center gap-1.5">
                  <Caret open={open} />
                  <span className="truncate text-sm font-medium text-kumo-default">
                    {p.name}
                  </span>
                </span>
                <Badge variant={statusBadge[p.status]} appearance="dot">
                  {p.status}
                </Badge>
              </button>
              <div className="flex items-center justify-between gap-2 px-3 pb-2.5">
                <CardMeta p={p} fetchedAt={fetchedAt} now={now} />
                <ActionsMenu p={p} size="base" {...actions} />
              </div>
              {open && (
                <div className="border-t border-kumo-hairline px-3 py-3">
                  <ProcessDetails
                    p={p}
                    fetchedAt={fetchedAt}
                    now={now}
                    onLogs={onLogs}
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* Desktop: fixed-layout table (content can never reflow columns). */}
      <div className="hidden overflow-x-auto rounded-xl bg-kumo-base ring ring-kumo-line md:block">
        <Table layout="fixed">
          <Table.Header variant="compact">
            <Table.Row>
              <Table.Head>Process</Table.Head>
              <Table.Head className="w-[130px]">Status</Table.Head>
              <Table.Head className="w-[110px]">Uptime</Table.Head>
              <Table.Head className="w-[90px]">Restarts</Table.Head>
              <Table.Head className="w-[120px]">Last crash</Table.Head>
              <Table.Head className="w-[60px]" aria-label="Actions" />
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {processes.map((p) => {
              const open = expanded === p.name;
              const crashFresh =
                p.last_crash_at != null && now / 1000 - p.last_crash_at < 300;
              return (
                <Fragment key={p.name}>
                  <Table.Row
                    onClick={() => toggle(p.name)}
                    aria-expanded={open}
                    className="cursor-pointer hover:bg-kumo-tint"
                  >
                    <Table.Cell className={stripe(p) || undefined}>
                      <div className="flex min-w-0 items-center gap-1.5">
                        <Caret open={open} />
                        <span className="truncate text-sm font-medium text-kumo-default">
                          {p.name}
                        </span>
                      </div>
                    </Table.Cell>
                    <Table.Cell>
                      <Badge variant={statusBadge[p.status]} appearance="dot">
                        {p.status}
                      </Badge>
                    </Table.Cell>
                    <Table.Cell>
                      <span
                        className={`text-sm tabular-nums ${
                          p.uptime_secs != null
                            ? "text-kumo-default"
                            : "text-kumo-subtle"
                        }`}
                      >
                        {displayUptime(p, fetchedAt, now)}
                      </span>
                    </Table.Cell>
                    <Table.Cell>
                      <span
                        className={`text-sm tabular-nums ${
                          p.restarts > 0
                            ? "text-kumo-warning"
                            : "text-kumo-default"
                        }`}
                      >
                        {p.restarts}
                      </span>
                    </Table.Cell>
                    <Table.Cell>
                      <span
                        className={`text-sm ${
                          crashFresh ? "text-kumo-danger" : "text-kumo-subtle"
                        }`}
                      >
                        {p.last_crash_at != null
                          ? formatAgo(p.last_crash_at)
                          : "—"}
                      </span>
                    </Table.Cell>
                    <Table.Cell>
                      <div
                        className="flex justify-end"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <ActionsMenu p={p} {...actions} />
                      </div>
                    </Table.Cell>
                  </Table.Row>
                  {open && (
                    <Table.Row>
                      <Table.Cell
                        colSpan={6}
                        className={`bg-kumo-recessed ${stripe(p)}`}
                      >
                        <div className="px-1 py-1">
                          <ProcessDetails
                            p={p}
                            fetchedAt={fetchedAt}
                            now={now}
                            onLogs={onLogs}
                          />
                        </div>
                      </Table.Cell>
                    </Table.Row>
                  )}
                </Fragment>
              );
            })}
          </Table.Body>
        </Table>
      </div>
    </>
  );
}
