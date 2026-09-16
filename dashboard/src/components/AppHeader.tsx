import { Badge, Button, Text } from "@cloudflare/kumo";
import { PlusIcon } from "@phosphor-icons/react";
import { ThemeToggle } from "./ThemeToggle";

export function AppHeader({
  isDark,
  onToggleTheme,
  daemonError,
  connecting,
  startEnabled,
  daemonHost,
  onStart,
}: {
  isDark: boolean;
  onToggleTheme: () => void;
  daemonError: string | null;
  connecting: boolean;
  startEnabled: boolean;
  daemonHost: string;
  onStart: () => void;
}) {
  return (
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
            ) : connecting ? (
              <Badge variant="neutral" appearance="dot">
                Connecting
              </Badge>
            ) : (
              <Badge variant="success" appearance="dot">
                Connected
              </Badge>
            )}
          </span>
          <ThemeToggle isDark={isDark} onToggle={onToggleTheme} />
          {startEnabled && (
            <>
              <span className="hidden h-4 w-px bg-kumo-line sm:block" />
              <span className="hidden sm:block">
                <Button
                  variant="primary"
                  icon={<PlusIcon size={16} />}
                  onClick={onStart}
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
                  onClick={onStart}
                />
              </span>
            </>
          )}
        </div>
      </div>
    </header>
  );
}
