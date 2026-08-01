export type ProcessStatus = "running" | "stopped" | "crashed" | "backoff";

export interface ProcessInfo {
  name: string;
  command: string;
  cwd: string;
  status: ProcessStatus;
  pid: number | null;
  uptime_secs: number | null;
  restarts: number;
  last_exit_code: number | null;
  last_crash_at: number | null;
  created_at: number;
}

export interface LogLine {
  name: string;
  stream: "stdout" | "stderr";
  ts: string;
  line: string;
}
