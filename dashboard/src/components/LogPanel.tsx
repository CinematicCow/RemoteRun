import { Suspense, lazy } from "react";
import { Dialog, Loader } from "@cloudflare/kumo";
import { Drawer } from "vaul";
import { useIsMobile } from "../useMediaQuery";

/* Log viewing pulls in react-virtuoso; keep it out of the initial bundle —
   it only loads the first time a log panel opens. */
const LogViewer = lazy(() =>
  import("./LogViewer").then((m) => ({ default: m.LogViewer })),
);

function LogViewerFallback() {
  return (
    <div className="flex h-[52vh] items-center justify-center">
      <Loader className="text-kumo-subtle" />
    </div>
  );
}

/** Hosts the log viewer in a bottom drawer on mobile and a dialog on desktop.
 *  `name` is kept mounted through the close animation by the caller. */
export function LogPanel({
  name,
  open,
  onClose,
}: {
  name: string | null;
  open: boolean;
  onClose: () => void;
}) {
  const isMobile = useIsMobile();
  if (isMobile) {
    return (
      <Drawer.Root open={open} onOpenChange={(next) => !next && onClose()}>
        <Drawer.Portal>
          <Drawer.Overlay className="fixed inset-0 z-40 bg-black/60" />
          <Drawer.Content
            aria-describedby={undefined}
            className="fixed inset-x-0 bottom-0 z-50 flex h-[94dvh] flex-col rounded-t-2xl bg-kumo-base shadow-2xl ring ring-kumo-line outline-none"
          >
            <Drawer.Handle className="mx-auto mt-2.5 mb-1 h-1 w-9 shrink-0 rounded-full bg-kumo-fill" />
            <div className="flex min-h-0 flex-1 flex-col px-4 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]">
              {name && (
                <Suspense fallback={<LogViewerFallback />}>
                  <LogViewer name={name} chrome="drawer" onClose={onClose} />
                </Suspense>
              )}
            </div>
          </Drawer.Content>
        </Drawer.Portal>
      </Drawer.Root>
    );
  }

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <Dialog size="xl" className="p-6">
        {name && (
          <Suspense fallback={<LogViewerFallback />}>
            <LogViewer name={name} />
          </Suspense>
        )}
      </Dialog>
    </Dialog.Root>
  );
}
