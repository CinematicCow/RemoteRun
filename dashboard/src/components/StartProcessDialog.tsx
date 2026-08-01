import { useState } from "react";
import { Banner, Button, Dialog, Input } from "@cloudflare/kumo";
import { WarningCircleIcon, XIcon } from "@phosphor-icons/react";
import { api } from "../api";

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function StartProcessDialog({
  open,
  onOpenChange,
  onStarted,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onStarted: (name: string) => void;
}) {
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [starting, setStarting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const reset = () => {
    setName("");
    setCommand("");
    setFormError(null);
  };

  const startProcess = async () => {
    const trimmedName = name.trim();
    const trimmedCommand = command.trim();
    if (!trimmedName || !trimmedCommand) {
      setFormError("Both a name and a command are required.");
      return;
    }
    setFormError(null);
    setStarting(true);
    try {
      await api.start(trimmedName, trimmedCommand);
      onStarted(trimmedName);
      onOpenChange(false);
      reset();
    } catch (e) {
      setFormError(errorMessage(e));
    } finally {
      setStarting(false);
    }
  };

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(next) => {
        onOpenChange(next);
        if (!next) setFormError(null);
      }}
    >
      <Dialog className="p-6">
        <div className="mb-4 flex items-start justify-between gap-4">
          <div className="grid gap-1">
            <Dialog.Title className="text-lg font-semibold">
              Start a process
            </Dialog.Title>
            <Dialog.Description className="text-sm text-kumo-subtle">
              The daemon runs the command and keeps it alive.
            </Dialog.Description>
          </div>
          <Dialog.Close
            render={(props) => (
              <Button
                {...props}
                variant="ghost"
                shape="square"
                size="sm"
                icon={<XIcon size={16} />}
                aria-label="Close"
              />
            )}
          />
        </div>

        <div className="flex flex-col gap-3">
          {formError && (
            <Banner
              size="sm"
              variant="error"
              icon={<WarningCircleIcon weight="fill" size={16} />}
              title={formError}
            />
          )}
          <Input
            label="Name"
            placeholder="api"
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
          />
          <Input
            label="Command"
            placeholder="bun start"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") startProcess();
            }}
          />
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <Dialog.Close
            render={(props) => (
              <Button variant="secondary" {...props}>
                Cancel
              </Button>
            )}
          />
          <Button variant="primary" loading={starting} onClick={startProcess}>
            Start
          </Button>
        </div>
      </Dialog>
    </Dialog.Root>
  );
}
