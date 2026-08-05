import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Badge,
  Banner,
  Button,
  DeleteResource,
  Dialog,
  Empty,
  InputGroup,
  Loader,
  Text,
  useKumoToastManager,
} from "@cloudflare/kumo";
import {
  MagnifyingGlassIcon,
  PlusIcon,
  TerminalWindowIcon,
  WarningCircleIcon,
  XIcon,
} from "@phosphor-icons/react";
import { Drawer } from "vaul";
import { api } from "./api";
import type { ProcessInfo, ProcessStatus } from "./types";
import { useColorMode } from "./useColorMode";
import { useIsMobile } from "./useMediaQuery";
import { ThemeToggle } from "./components/ThemeToggle";
import { StatCards, type StatusFilter } from "./components/StatCards";
import { ProcessTable } from "./components/ProcessTable";
import { StartProcessDialog } from "./components/StartProcessDialog";

/* Log viewing pulls in react-virtuoso; keep it out of the initial bundle —
   it only loads the first time a log panel opens. */
const LogViewer = lazy(() =>
  import("./components/LogViewer").then((m) => ({ default: m.LogViewer })),
);

function LogViewerFallback() {
  return (
    <div className="flex h-[52vh] items-center justify-center">
      <Loader className="text-kumo-subtle" />
    </div>
  );
}

const POLL_MS = 2000;

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export default function App() {
  const { isDark, toggle } = useColorMode();
  const toastManager = useKumoToastManager();

  const [processes, setProcesses] = useState<ProcessInfo[] | null>(null);
  /* A ref, not state: only the per-second uptime leaves read it, and they
     re-render on their own clock — polls shouldn't re-render the app. */
  const fetchedAtRef = useRef(0);
  const [startEnabled, setStartEnabled] = useState(false);
  const [daemonError, setDaemonError] = useState<string | null>(null);
  const daemonDown = useRef(false);
  const failures = useRef(0);

  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");

  const [selectedLogs, setSelectedLogs] = useState<string | null>(null);
  /* Keep the last-opened process mounted through the close animation. */
  const shownLogs = useRef<string | null>(null);
  if (selectedLogs) shownLogs.current = selectedLogs;
  const isMobile = useIsMobile();
  const [pendingRemove, setPendingRemove] = useState<ProcessInfo | null>(null);
  const [removing, setRemoving] = useState(false);
  const [removeError, setRemoveError] = useState<string | undefined>();
  const [startOpen, setStartOpen] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const { processes, startEnabled } = await api.ps();
      setStartEnabled(startEnabled);
      fetchedAtRef.current = Date.now();
      /* Identical snapshot (e.g. everything stopped) → keep the previous
         array so nothing below re-renders. */
      setProcesses((prev) =>
        prev != null && JSON.stringify(prev) === JSON.stringify(processes)
          ? prev
          : processes,
      );
      setDaemonError(null);
      daemonDown.current = false;
      failures.current = 0;
    } catch (e) {
      failures.current += 1;
      const msg = errorMessage(e);
      setDaemonError(msg);
      /* Toast once per outage, not every poll. */
      if (!daemonDown.current) {
        daemonDown.current = true;
        toastManager.add({
          variant: "error",
          title: "Can't reach the rr daemon",
          description: msg,
        });
      }
    }
  }, [toastManager]);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      await refresh();
      if (!alive) return;
      /* Back off while the daemon is unreachable (2s → 15s max), so a
         downed daemon doesn't spam the vite proxy log or the daemon
         itself as it comes back. Snaps back to 2s on first success. */
      const delay = Math.min(POLL_MS * 2 ** failures.current, 15_000);
      timer = setTimeout(tick, delay);
    };
    tick();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [refresh]);

  const act = async (fn: () => Promise<unknown>, failTitle: string) => {
    try {
      await fn();
      await refresh();
    } catch (e) {
      toastManager.add({
        variant: "error",
        title: failTitle,
        description: errorMessage(e),
      });
    }
  };

  const confirmRemove = async () => {
    if (!pendingRemove) return;
    setRemoving(true);
    setRemoveError(undefined);
    try {
      await api.remove(pendingRemove.name);
      await refresh();
      toastManager.add({
        variant: "success",
        title: `Removed ${pendingRemove.name}`,
      });
      if (selectedLogs === pendingRemove.name) setSelectedLogs(null);
      setPendingRemove(null);
    } catch (e) {
      setRemoveError(errorMessage(e));
    } finally {
      setRemoving(false);
    }
  };

  const counts = useMemo(() => {
    const c: Record<ProcessStatus, number> = {
      running: 0,
      crashed: 0,
      backoff: 0,
      stopped: 0,
    };
    for (const p of processes ?? []) c[p.status]++;
    return c;
  }, [processes]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (processes ?? []).filter((p) => {
      if (statusFilter !== "all" && p.status !== statusFilter) return false;
      if (!q) return true;
      return (
        p.name.toLowerCase().includes(q) || p.command.toLowerCase().includes(q)
      );
    });
  }, [processes, query, statusFilter]);

  const list = processes ?? [];
  const daemonHost = window.location.host;

  return (
    <div className="min-h-screen bg-kumo-canvas">
      <header className="sticky top-0 z-10 border-b border-kumo-line bg-kumo-canvas/85 backdrop-blur">
        <div className="mx-auto flex max-w-6xl items-center gap-2.5 px-4 py-2.5 sm:gap-3 sm:px-6">
          <img src="/logo.png" alt="" className="h-8 w-8 rounded-md" />
          <div className="grid min-w-0">
            <Text as="h1" variant="heading3">
              Remote Run
            </Text>
            <span className="hidden font-mono text-xs text-kumo-subtle sm:block">
              {daemonHost}
            </span>
          </div>

          <div className="ml-auto flex items-center gap-2">
            <span className="hidden sm:inline-flex">
              {daemonError ? (
                <Badge variant="error" appearance="dot">
                  Unreachable
                </Badge>
              ) : processes === null ? (
                <Badge variant="neutral" appearance="dot">
                  Connecting
                </Badge>
              ) : (
                <Badge variant="success" appearance="dot">
                  Connected
                </Badge>
              )}
            </span>
            <ThemeToggle isDark={isDark} onToggle={toggle} />
            {startEnabled && (
              <>
                <span className="hidden h-4 w-px bg-kumo-line sm:block" />
                <span className="hidden sm:block">
                  <Button
                    variant="primary"
                    icon={<PlusIcon size={16} />}
                    onClick={() => setStartOpen(true)}
                  >
                    Start process
                  </Button>
                </span>
                <span className="sm:hidden">
                  <Button
                    variant="primary"
                    shape="square"
                    icon={<PlusIcon size={16} />}
                    aria-label="Start process"
                    onClick={() => setStartOpen(true)}
                  />
                </span>
              </>
            )}
          </div>
        </div>
      </header>

      <main className="mx-auto flex max-w-6xl flex-col gap-5 px-4 py-6 sm:px-6">
        {daemonError && (
          <Banner
            variant="error"
            icon={<WarningCircleIcon weight="fill" size={16} />}
            title="Can't reach the rr daemon"
            description={`${daemonError} — retrying in the background.`}
          />
        )}

        <StatCards
          counts={counts}
          active={statusFilter}
          onSelect={(s) =>
            setStatusFilter((cur) => (cur === s ? "all" : s))
          }
        />

        <div className="flex flex-wrap items-center gap-3">
          <InputGroup className="relative z-0 w-full sm:ml-auto sm:w-72">
            <InputGroup.Addon>
              <MagnifyingGlassIcon size={16} />
            </InputGroup.Addon>
            <InputGroup.Input
              aria-label="Filter processes"
              placeholder="Filter by name or command"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") setQuery("");
              }}
            />
            {query && (
              <InputGroup.Addon align="end">
                <Button
                  variant="ghost"
                  shape="square"
                  size="xs"
                  icon={<XIcon size={14} />}
                  aria-label="Clear filter"
                  onClick={() => setQuery("")}
                />
              </InputGroup.Addon>
            )}
          </InputGroup>
        </div>

        {processes === null ? (
          <div className="flex items-center justify-center gap-3 rounded-xl bg-kumo-base px-6 py-20 ring ring-kumo-line">
            <Loader className="text-kumo-subtle" />
            <span className="text-sm text-kumo-subtle">
              {daemonError
                ? "Daemon unreachable — retrying…"
                : "Connecting to the daemon…"}
            </span>
          </div>
        ) : list.length === 0 ? (
          <div className="rounded-xl bg-kumo-base ring ring-kumo-line">
            <Empty
              size="sm"
              icon={<TerminalWindowIcon size={28} />}
              title="No processes yet"
              description={
                startEnabled
                  ? "Start one here, or from a terminal:"
                  : "Start one from a terminal:"
              }
              commandLine='rr -n api "bun start"'
              contents={
                startEnabled && (
                  <Button
                    variant="primary"
                    icon={<PlusIcon size={16} />}
                    onClick={() => setStartOpen(true)}
                  >
                    Start a process
                  </Button>
                )
              }
            />
          </div>
        ) : filtered.length === 0 ? (
          <div className="rounded-xl bg-kumo-base ring ring-kumo-line">
            <Empty
              size="sm"
              icon={<MagnifyingGlassIcon size={28} />}
              title="No matching processes"
              description="Nothing matches the current filters."
              contents={
                <Button
                  variant="secondary"
                  onClick={() => {
                    setQuery("");
                    setStatusFilter("all");
                  }}
                >
                  Clear filters
                </Button>
              }
            />
          </div>
        ) : (
          <ProcessTable
            processes={filtered}
            fetchedAtRef={fetchedAtRef}
            onLogs={setSelectedLogs}
            onRestart={(name) =>
              act(() => api.restart(name), `Couldn't restart ${name}`)
            }
            onStop={(name) =>
              act(() => api.stop(name), `Couldn't stop ${name}`)
            }
            onRemove={(p) => {
              setRemoveError(undefined);
              setPendingRemove(p);
            }}
          />
        )}
      </main>

      <StartProcessDialog
        open={startOpen}
        onOpenChange={setStartOpen}
        onStarted={(name) => {
          refresh();
          toastManager.add({ variant: "success", title: `Started ${name}` });
          /* First-success moment: land the user on live logs. */
          setSelectedLogs(name);
        }}
      />

      {isMobile ? (
        <Drawer.Root
          open={selectedLogs !== null}
          onOpenChange={(open) => !open && setSelectedLogs(null)}
        >
          <Drawer.Portal>
            <Drawer.Overlay className="fixed inset-0 z-40 bg-black/60" />
            <Drawer.Content
              aria-describedby={undefined}
              className="fixed inset-x-0 bottom-0 z-50 flex h-[94dvh] flex-col rounded-t-2xl bg-kumo-base shadow-2xl ring ring-kumo-line outline-none"
            >
              <Drawer.Handle className="mx-auto mt-2.5 mb-1 h-1 w-9 shrink-0 rounded-full bg-kumo-fill" />
              <div className="flex min-h-0 flex-1 flex-col px-4 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]">
                {shownLogs.current && (
                  <Suspense fallback={<LogViewerFallback />}>
                    <LogViewer
                      name={shownLogs.current}
                      chrome="drawer"
                      onClose={() => setSelectedLogs(null)}
                    />
                  </Suspense>
                )}
              </div>
            </Drawer.Content>
          </Drawer.Portal>
        </Drawer.Root>
      ) : (
        <Dialog.Root
          open={selectedLogs !== null}
          onOpenChange={(open) => !open && setSelectedLogs(null)}
        >
          <Dialog size="xl" className="p-6">
            {shownLogs.current && (
              <Suspense fallback={<LogViewerFallback />}>
                <LogViewer name={shownLogs.current} />
              </Suspense>
            )}
          </Dialog>
        </Dialog.Root>
      )}

      <DeleteResource
        open={pendingRemove !== null}
        onOpenChange={(open) => {
          if (!open) {
            setPendingRemove(null);
            setRemoveError(undefined);
          }
        }}
        resourceType="process"
        resourceName={pendingRemove?.name ?? ""}
        onDelete={confirmRemove}
        isDeleting={removing}
        errorMessage={removeError}
      />
    </div>
  );
}
