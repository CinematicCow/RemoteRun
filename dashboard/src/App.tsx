import { useCallback, useEffect, useState } from "react";
import {
  Badge,
  Banner,
  Button,
  Empty,
  Input,
  LayerCard,
  Table,
  Text,
} from "@cloudflare/kumo";
import {
  ArrowClockwiseIcon,
  PlayIcon,
  StackIcon,
  StopIcon,
  TerminalWindowIcon,
  TrashIcon,
} from "@phosphor-icons/react";
import { api, formatAgo, formatUptime } from "./api";
import type { ProcessInfo, ProcessStatus } from "./types";
import { LogViewer } from "./LogViewer";

const POLL_MS = 2000;

const statusBadge: Record<
  ProcessStatus,
  "success" | "neutral" | "error" | "warning"
> = {
  running: "success",
  stopped: "neutral",
  crashed: "error",
  backoff: "warning",
};

export default function App() {
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [starting, setStarting] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const { processes } = await api.ps();
      setProcesses(processes);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const act = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const startProcess = async () => {
    if (!name.trim() || !command.trim()) {
      setError("both a name and a command are required");
      return;
    }
    setStarting(true);
    await act(() => api.start(name.trim(), command.trim()));
    setStarting(false);
    setName("");
    setCommand("");
  };

  const removeProcess = (proc: ProcessInfo) =>
    act(async () => {
      await api.remove(proc.name);
      if (selected === proc.name) {
        setSelected(null);
      }
    });

  return (
    <div className="app">
      <header className="app-header">
        <StackIcon size={24} weight="duotone" />
        <Text as="h1" variant="heading2">
          rr
        </Text>
        <Text size="sm" variant="secondary">
          remote run — process manager
        </Text>
      </header>

      <div className="start-form">
        <Input
          className="name-input"
          size="sm"
          placeholder="name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <Input
          className="command-input"
          size="sm"
          placeholder='command, e.g. "bun start"'
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") startProcess();
          }}
        />
        <Button
          variant="primary"
          size="sm"
          icon={PlayIcon}
          loading={starting}
          onClick={startProcess}
        >
          Start
        </Button>
      </div>

      {error && <Banner variant="error" title={error} />}

      <LayerCard className="p-0">
        {processes.length === 0 ? (
          <Empty
            size="sm"
            icon={<TerminalWindowIcon size={28} />}
            title="No processes yet"
            description="Start one above, or from a terminal:"
            commandLine='rr --name api "bun start"'
          />
        ) : (
          <Table>
            <Table.Header variant="compact">
              <Table.Row>
                <Table.Head>Name</Table.Head>
                <Table.Head>Status</Table.Head>
                <Table.Head>Pid</Table.Head>
                <Table.Head>Uptime</Table.Head>
                <Table.Head>Restarts</Table.Head>
                <Table.Head>Last exit</Table.Head>
                <Table.Head>Last crash</Table.Head>
                <Table.Head aria-label="Actions" />
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {processes.map((p) => (
                <Table.Row
                  key={p.name}
                  variant={selected === p.name ? "selected" : undefined}
                >
                  <Table.Cell>
                    <div className="flex flex-col">
                      <Text size="sm" bold>
                        {p.name}
                      </Text>
                      <span className="mono muted truncated">{p.command}</span>
                    </div>
                  </Table.Cell>
                  <Table.Cell>
                    <Badge variant={statusBadge[p.status]} appearance="dot">
                      {p.status}
                    </Badge>
                  </Table.Cell>
                  <Table.Cell>
                    <span className="mono">{p.pid ?? "—"}</span>
                  </Table.Cell>
                  <Table.Cell>
                    <Text size="sm">
                      {p.uptime_secs != null ? formatUptime(p.uptime_secs) : "—"}
                    </Text>
                  </Table.Cell>
                  <Table.Cell>
                    <Text size="sm">{p.restarts}</Text>
                  </Table.Cell>
                  <Table.Cell>
                    <span className="mono">{p.last_exit_code ?? "—"}</span>
                  </Table.Cell>
                  <Table.Cell>
                    <Text size="sm" variant="secondary">
                      {p.last_crash_at != null ? formatAgo(p.last_crash_at) : "—"}
                    </Text>
                  </Table.Cell>
                  <Table.Cell>
                    <div className="row-actions">
                      <Button
                        variant="ghost"
                        size="xs"
                        icon={TerminalWindowIcon}
                        onClick={() =>
                          setSelected(selected === p.name ? null : p.name)
                        }
                      >
                        Logs
                      </Button>
                      <Button
                        variant="ghost"
                        size="xs"
                        icon={ArrowClockwiseIcon}
                        onClick={() => act(() => api.restart(p.name))}
                      >
                        Restart
                      </Button>
                      {p.status === "running" || p.status === "backoff" ? (
                        <Button
                          variant="ghost"
                          size="xs"
                          icon={StopIcon}
                          onClick={() => act(() => api.stop(p.name))}
                        >
                          Stop
                        </Button>
                      ) : (
                        <Button
                          variant="ghost"
                          size="xs"
                          icon={TrashIcon}
                          onClick={() => removeProcess(p)}
                        >
                          Remove
                        </Button>
                      )}
                    </div>
                  </Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </Table>
        )}
      </LayerCard>

      {selected && (
        <LogViewer name={selected} onClose={() => setSelected(null)} />
      )}
    </div>
  );
}
