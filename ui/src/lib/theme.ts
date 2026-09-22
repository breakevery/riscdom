/**
 * Theme choice (v0.4 #11a).
 *
 * Dependency-free so `ui/scripts/probe-ui-theme.mjs` can import the shipped rules:
 * what comes next in the three-state cycle, what "system" resolves to right now,
 * and the one place that writes `data-theme`. Its one import is the i18n registry,
 * itself dependency-free (v0.8 batch 2).
 */
import { t } from "../i18n/index.ts";

export type Theme = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

/** The cycle order the UI offers. */
export const THEMES: Theme[] = ["light", "dark", "system"];

/** Parse what came back from settings; anything unknown means "system". */
export function parseTheme(raw: unknown): Theme {
  return raw === "light" || raw === "dark" || raw === "system" ? raw : "system";
}

/** The three-state cycle: light → dark → system → light. */
export function nextTheme(current: Theme): Theme {
  const index = THEMES.indexOf(parseTheme(current));
  return THEMES[(index + 1) % THEMES.length];
}

/** Human label for a choice. */
export function themeLabel(theme: Theme): string {
  switch (parseTheme(theme)) {
    case "light":
      return t("theme.light");
    case "dark":
      return t("theme.dark");
    default:
      return t("theme.system");
  }
}

/** What a choice means while the OS prefers `systemPrefersDark`. */
export function resolveTheme(theme: Theme, systemPrefersDark: boolean): ResolvedTheme {
  const parsed = parseTheme(theme);
  if (parsed === "system") return systemPrefersDark ? "dark" : "light";
  return parsed;
}

/** Apply a choice to a document element: the only writer of `data-theme`. */
export function applyTheme(
  theme: Theme,
  root: { setAttribute(name: string, value: string): void },
  systemPrefersDark: boolean,
): ResolvedTheme {
  const resolved = resolveTheme(theme, systemPrefersDark);
  root.setAttribute("data-theme", resolved);
  return resolved;
}

/** One line for the settings page. */
export function themeSummary(theme: Theme, resolved: ResolvedTheme): string {
  const parsed = parseTheme(theme);
  const name = resolved === "dark" ? t("theme.dark") : t("theme.light");
  return parsed === "system"
    ? t("theme.summary.system", { name })
    : t("theme.summary.fixed", { name });
}

/**
 * Does the OS prefer dark right now.
 *
 * `matchMedia` is absent outside a browser; the app's own palette was dark first,
 * so "unknown" stays dark rather than flashing white.
 */
export function systemPrefersDark(win: {
  matchMedia?: (query: string) => { matches: boolean };
}): boolean {
  try {
    // NOTE: `win.matchMedia?.(q).matches` would short-circuit the *whole* chain
    // and yield `undefined` (→ false) when `matchMedia` is missing, which is the
    // opposite of what "unknown" should mean here.
    const query = win.matchMedia?.("(prefers-color-scheme: dark)");
    if (!query) return true;
    return Boolean(query.matches);
  } catch {
    return true;
  }
}
