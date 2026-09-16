import { Fragment, useState, type RefObject } from "react";
import { Badge, Table } from "@cloudflare/kumo";
import { CaretRightIcon } from "@phosphor-icons/react";
import { formatAgo } from "../api";
import { statusBadge } from "../status";
import type { ProcessInfo } from "../types";
import { ActionsMenu, type ProcessActions } from "./ProcessActions";
import { LiveUptime, ProcessDetails } from "./ProcessDetails";

interface Props extends ProcessActions {
  processes: ProcessInfo[];
  /** When the current process snapshot was fetched (for live uptime ticks). */
  fetchedAtRef: RefObject<number>;
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

export function ProcessTable({
  processes,
  fetchedAtRef,
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
              className="rounded-xl bg-kumo-base ring ring-kumo-line"
            >
              <div className="flex items-stretch">
                <button
                  type="button"
                  onClick={() => toggle(p.name)}
                  aria-expanded={open}
                  className="flex min-w-0 flex-1 items-center justify-between gap-2 py-3.5 pl-3.5 text-left"
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
                <div className="flex items-center px-1.5">
                  <ActionsMenu p={p} size="base" {...actions} />
                </div>
              </div>
              {open && (
                <div className="border-t border-kumo-hairline px-3.5 py-3">
                  <ProcessDetails
                    p={p}
                    fetchedAtRef={fetchedAtRef}
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
              return (
                <Fragment key={p.name}>
                  <Table.Row
                    onClick={() => toggle(p.name)}
                    aria-expanded={open}
                    className="cursor-pointer hover:bg-kumo-tint"
                  >
                    <Table.Cell>
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
                        <LiveUptime p={p} fetchedAtRef={fetchedAtRef} />
                      </span>
                    </Table.Cell>
                    <Table.Cell>
                      <span
                        className={`text-sm tabular-nums ${
                          p.restarts > 0
                            ? "text-kumo-default"
                            : "text-kumo-subtle"
                        }`}
                      >
                        {p.restarts}
                      </span>
                    </Table.Cell>
                    <Table.Cell>
                      <span className="text-sm text-kumo-subtle">
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
                      <Table.Cell colSpan={6} className="bg-kumo-recessed">
                        <div className="px-1 py-1">
                          <ProcessDetails
                            p={p}
                            fetchedAtRef={fetchedAtRef}
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
