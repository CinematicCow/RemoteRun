import type { LogLine, ProcessInfo } from "./types";

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, init);
  const body = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(body.error ?? `${res.status} ${res.statusText}`);
  }
  return body as T;
}

export const api = {
  ps: () => request<{ processes: ProcessInfo[] }>("/api/ps"),

  start: (name: string, command: string) =>
    request<{ process: ProcessInfo }>("/api/start", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name, command }),
    }),

  stop: (name: string) =>
    request<{ process: ProcessInfo }>(
      `/api/processes/${encodeURIComponent(name)}/stop`,
      { method: "POST" },
    ),

  restart: (name: string) =>
    request<{ process: ProcessInfo }>(
      `/api/processes/${encodeURIComponent(name)}/restart`,
      { method: "POST" },
    ),

  remove: (name: string) =>
    request<{ ok: boolean }>(`/api/processes/${encodeURIComponent(name)}`, {
      method: "DELETE",
    }),

  history: (name: string, lines = 200) =>
    request<{ lines: LogLine[] }>(
      `/api/logs/${encodeURIComponent(name)}?lines=${lines}`,
    ),

  streamUrl: (name: string) =>
    `/api/logs/${encodeURIComponent(name)}/stream`,
};

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
