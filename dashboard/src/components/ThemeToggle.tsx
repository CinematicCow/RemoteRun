import { Button, Tooltip } from "@cloudflare/kumo";
import { MoonIcon, SunIcon } from "@phosphor-icons/react";

export function ThemeToggle({
  isDark,
  onToggle,
}: {
  isDark: boolean;
  onToggle: () => void;
}) {
  const label = isDark ? "Switch to light mode" : "Switch to dark mode";
  return (
    <Tooltip
      content={label}
      render={
        <Button
          variant="ghost"
          shape="square"
          onClick={onToggle}
          aria-label={label}
          icon={
            <span
              key={isDark ? "sun" : "moon"}
              className="rr-icon-swap inline-flex"
            >
              {isDark ? <SunIcon size={16} /> : <MoonIcon size={16} />}
            </span>
          }
        />
      }
    />
  );
}
