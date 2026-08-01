import { useEffect, useState } from "react";

/**
 * Re-renders the component every `interval` ms with the current timestamp.
 * Used to tick live uptimes between daemon polls.
 */
export function useNow(interval = 1000): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), interval);
    return () => clearInterval(id);
  }, [interval]);

  return now;
}
