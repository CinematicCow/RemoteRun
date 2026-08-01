import { useRef, useState } from "react";
import { Button, Tooltip } from "@cloudflare/kumo";
import { CheckIcon, CopyIcon } from "@phosphor-icons/react";

/**
 * Icon-only copy-to-clipboard button with a transient check confirmation.
 * `getText` is evaluated at click time so callers can copy live state.
 */
export function CopyButton({
  getText,
  label,
  size = "base",
}: {
  getText: () => string;
  label: string;
  size?: "xs" | "sm" | "base";
}) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(null);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(getText());
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  };

  return (
    <Tooltip
      content={copied ? "Copied" : label}
      render={
        <Button
          variant="ghost"
          shape="square"
          size={size}
          icon={
            copied ? (
              <CheckIcon size={16} className="text-kumo-success" />
            ) : (
              <CopyIcon size={16} />
            )
          }
          aria-label={copied ? "Copied" : label}
          onClick={copy}
        />
      }
    />
  );
}
