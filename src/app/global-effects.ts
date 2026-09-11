import { useEffect } from "react";
import { toggleDevtools } from "../api/client";
import { isBrowserDevelopment } from "../lib/runtime";

/** Dev-build debug affordance: F12 toggles the WebView inspector. Absent
 * from production bundles, where the WebView inspector is disabled. */
export function useDevtoolsShortcut(): void {
  useEffect(() => {
    if (!import.meta.env.DEV || isBrowserDevelopment) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "F12") {
        event.preventDefault();
        void toggleDevtools().catch(() => {});
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}

/** Tracks input modality for keyboard-only action affordances. */
export function useKeyboardFocusMarker(): void {
  useEffect(() => {
    const root = document.documentElement;
    const markKeyboard = (event: KeyboardEvent) => {
      if (event.key === "Tab" || event.key === " " || event.key.startsWith("Arrow")) {
        root.dataset.focusSource = "key";
      }
    };
    const markPointer = () => {
      delete root.dataset.focusSource;
    };
    window.addEventListener("keydown", markKeyboard);
    window.addEventListener("pointerdown", markPointer, true);
    return () => {
      window.removeEventListener("keydown", markKeyboard);
      window.removeEventListener("pointerdown", markPointer, true);
    };
  }, []);
}
