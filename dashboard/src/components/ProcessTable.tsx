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

function Meta({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
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

  return (
    <Table>
      <Table.Header variant="compact">
        <Table.Row>
          <Table.Head>Process</Table.Head>
          <Table.Head>Status</Table.Head>
          <Table.Head>Uptime</Table.Head>
          <Table.Head className="hidden md:table-cell">Restarts</Table.Head>
          <Table.Head className="hidden lg:table-cell">Last crash</Table.Head>
          <Table.Head aria-label="Actions" />
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {processes.map((p) => {
          const alive = p.status === "running" || p.status === "backoff";
          const open = expanded === p.name;
          /* Left edge stripe: instant "look here" for unhealthy rows. */
          const stripe =
            p.status === "crashed"
              ? "shadow-[inset_2px_0_0_var(--color-kumo-danger)]"
              : p.status === "backoff"
                ? "shadow-[inset_2px_0_0_var(--color-kumo-warning)]"
                : "";
          const crashFresh =
            p.last_crash_at != null && now / 1000 - p.last_crash_at < 300;
          return (
            <Fragment key={p.name}>
              <Table.Row
                onClick={() => setExpanded(open ? null : p.name)}
                aria-expanded={open}
                className="cursor-pointer hover:bg-kumo-tint"
              >
                <Table.Cell className={stripe || undefined}>
                  <div className="flex items-center gap-1.5">
                    <span className="flex h-lh items-center text-kumo-subtle">
                      <CaretRightIcon
                        size={12}
                        className={`transition-transform duration-150 ease-out ${
                          open ? "rotate-90" : ""
                        }`}
                      />
                    </span>
                    <span className="text-sm font-medium text-kumo-default">
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
                <Table.Cell className="hidden md:table-cell">
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
                <Table.Cell className="hidden lg:table-cell">
                  <span
                    className={`text-sm ${
                      crashFresh ? "text-kumo-danger" : "text-kumo-subtle"
                    }`}
                  >
                    {p.last_crash_at != null ? formatAgo(p.last_crash_at) : "—"}
                  </span>
                </Table.Cell>
                <Table.Cell>
                  <div
                    className="flex justify-end"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <DropdownMenu>
                      <DropdownMenu.Trigger
                        render={
                          <Button
                            variant="ghost"
                            shape="square"
                            size="sm"
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
                          <DropdownMenu.Item
                            variant="danger"
                            icon={StopIcon}
                            onClick={() => onStop(p.name)}
                          >
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
                  </div>
                </Table.Cell>
              </Table.Row>
              {open && (
                <Table.Row>
                  <Table.Cell
                    colSpan={6}
                    className={`bg-kumo-recessed ${stripe || ""}`}
                  >
                    <div className="grid gap-4 px-1 py-1">
                      <div className="grid gap-1.5">
                        <span className="text-xs text-kumo-subtle">
                          Command
                        </span>
                        <ClipboardText
                          text={p.command}
                          className="font-mono text-sm"
                        />
                      </div>
                      <div className="flex flex-wrap gap-x-10 gap-y-3">
                        <Meta label="Pid" value={p.pid != null ? String(p.pid) : "—"} />
                        <Meta
                          label="Last exit code"
                          value={
                            p.last_exit_code != null
                              ? String(p.last_exit_code)
                              : "—"
                          }
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
                  </Table.Cell>
                </Table.Row>
              )}
            </Fragment>
          );
        })}
      </Table.Body>
    </Table>
  );
}
