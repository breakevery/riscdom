import type { ReactNode } from "react";
import { isTauriRuntime } from "../api";

/**
 * Render its children only in the desktop shell (v0.9 D2b-4a).
 *
 * The browser is a **read-only board**: every control belongs to the desktop, and this
 * is how the screens say so — one wrapper around the control itself, rather than a
 * `readOnly` prop threaded through five tabs or a disabled button that would only fail
 * when pressed. `isTauriRuntime()` is asked at render time rather than passed in,
 * because the answer cannot change during a page's life.
 */
export function DesktopOnly({ children }: { children: ReactNode }) {
  return isTauriRuntime() ? <>{children}</> : null;
}

/**
 * The mirror: render its children only in the browser (v0.9 D2b-4a).
 *
 * Used for the one thing the browser has and the desktop does not — a note saying why
 * a control is missing.
 */
export function WebOnly({ children }: { children: ReactNode }) {
  return isTauriRuntime() ? null : <>{children}</>;
}
