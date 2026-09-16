import { Button, DropdownMenu } from "@cloudflare/kumo";
import {
  ArrowClockwiseIcon,
  DotsThreeVerticalIcon,
  StopIcon,
  TerminalWindowIcon,
  TrashIcon,
} from "@phosphor-icons/react";
import type { ProcessInfo } from "../types";

export interface ProcessActions {
  onLogs: (name: string) => void;
  onRestart: (name: string) => void;
  onStop: (name: string) => void;
  onRemove: (p: ProcessInfo) => void;
}

export function ActionsMenu({
  p,
  size = "sm",
  onLogs,
  onRestart,
  onStop,
  onRemove,
}: ProcessActions & { p: ProcessInfo; size?: "sm" | "base" }) {
  const alive = p.status === "running" || p.status === "backoff";
  return (
    <DropdownMenu>
      <DropdownMenu.Trigger
        render={
          <Button
            variant="ghost"
            shape="square"
            size={size}
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
          <DropdownMenu.Item icon={StopIcon} onClick={() => onStop(p.name)}>
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
  );
}
