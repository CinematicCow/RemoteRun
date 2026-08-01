import { useSyncExternalStore } from "react";

/** Reactive media query. `md` breakpoint (768px) gates the mobile layout. */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (cb) => {
      const mql = window.matchMedia(query);
      mql.addEventListener("change", cb);
      return () => mql.removeEventListener("change", cb);
    },
    () => window.matchMedia(query).matches,
    () => false,
  );
}

/** True below the Tailwind `md` breakpoint — cards & bottom-sheet territory. */
export function useIsMobile(): boolean {
  return useMediaQuery("(max-width: 767px)");
}
