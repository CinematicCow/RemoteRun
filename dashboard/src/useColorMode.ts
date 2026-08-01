import { useCallback, useState } from "react";

type ColorMode = "light" | "dark";
const STORAGE_KEY = "rr-color-mode";

function currentMode(): ColorMode {
  return document.documentElement.getAttribute("data-mode") === "dark"
    ? "dark"
    : "light";
}

function apply(mode: ColorMode) {
  document.documentElement.setAttribute("data-mode", mode);
  document.documentElement.style.colorScheme = mode;
  localStorage.setItem(STORAGE_KEY, mode);
}

export function useColorMode() {
  const [mode, setMode] = useState<ColorMode>(currentMode);

  const toggle = useCallback(() => {
    setMode((prev) => {
      const next = prev === "dark" ? "light" : "dark";
      apply(next);
      return next;
    });
  }, []);

  return { mode, isDark: mode === "dark", toggle };
}
