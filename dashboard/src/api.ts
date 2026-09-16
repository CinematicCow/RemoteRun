import type { LogLine, ProcessInfo } from "./types";

type RpcResponse =
  | { kind: "ok" }
  | { kind: "error"; message: string }
  | { kind: "processes"; processes: ProcessInfo[]; start_enabled: boolean }
  | { kind: "process"; process: ProcessInfo }
  | { kind: "log_history"; lines: LogLine[] };

async function rpc(req: unknown): Promise<RpcResponse> {
  const res = await fetch("/api/rpc", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(req),
  });
  const body = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(body.error ?? `${res.status} ${res.statusText}`);
  }
  if (body.kind === "error") {
    throw new Error(body.message);
  }
  return body as RpcResponse;
}

export const api = {
  ps: async () => {
    const res = await rpc({ cmd: "ps" });
    if (res.kind !== "processes") throw new Error("unexpected response");
    return { processes: res.processes, startEnabled: res.start_enabled };
  },

  start: (name: string, command: string) =>
    rpc({ cmd: "start", name, command, cwd: "" }),

  stop: (name: string) => rpc({ cmd: "stop", name }),

  restart: (name: string) => rpc({ cmd: "restart", name }),

  remove: (name: string) => rpc({ cmd: "remove", name }),

  history: async (name: string, lines = 200) => {
    const res = await rpc({ cmd: "logs", name, lines, follow: false });
    if (res.kind !== "log_history") throw new Error("unexpected response");
    return { lines: res.lines };
  },

  streamUrl: (name: string) => `/api/logs/${encodeURIComponent(name)}/stream`,
};

export function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86_400);
  const h = Math.floor((secs % 86_400) / 3_600);
  const m = Math.floor((secs % 3_600) / 60);
  const s = secs % 60;
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

export function formatAgo(unixTs: number): string {
  const secs = Math.max(0, Math.floor(Date.now() / 1000) - unixTs);
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3_600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86_400) return `${Math.floor(secs / 3_600)}h ago`;
  return `${Math.floor(secs / 86_400)}d ago`;
}
