/**
 * The self-built i18n facility (v0.7 batch 1): the current language and the one
 * place that turns a registry key into text.
 *
 * Dependency-free on purpose, like `lib/theme.ts` and `lib/runView.ts`: the node
 * probes and `scripts/check-ui-strings.mjs` import the shipped module directly
 * (Node strips the types), so nothing here may pull in React or the DOM. The
 * language the user chose lives in `appStore` (a manual override gets the last
 * word); this module holds what `t()` reads, so a non-React caller — `runView.ts`,
 * a probe — gets the same answer as the panel does.
 */
import { LANGUAGES, STRINGS } from "./strings.ts";
import type { Language, StringKey } from "./strings.ts";

export { LANGUAGES } from "./strings.ts";
export type { Language, StringKey } from "./strings.ts";

/** The language used when the system says nothing we recognise. */
export const DEFAULT_LANGUAGE: Language = "en";

/** A BCP-47 tag ("zh-CN", "en-US", …) as one of the registry's languages. */
export function detectLanguage(tag: unknown): Language {
  return typeof tag === "string" && tag.toLowerCase().startsWith("zh")
    ? "zh"
    : DEFAULT_LANGUAGE;
}

/** The system's language as the browser reports it; DOM-free callers get the default. */
function systemLanguage(): Language {
  const nav = (globalThis as { navigator?: { language?: unknown } }).navigator;
  return detectLanguage(nav?.language);
}

let current: Language = systemLanguage();
const listeners = new Set<() => void>();

/** The language `t()` reads right now. */
export function getLanguage(): Language {
  return current;
}

/** Choose a language by hand; it wins over the system from here on. */
export function setLanguage(next: Language): void {
  if (!LANGUAGES.includes(next) || next === current) return;
  current = next;
  for (const listener of listeners) listener();
}

/** Watch for a language change; returns the unsubscribe. */
export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Fill `{name}` placeholders; a placeholder without a value is left as written. */
function fill(template: string, params?: Record<string, string | number>): string {
  if (params === undefined) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : whole,
  );
}

/** The registry's text for `key`, in the current language. */
export function t(key: StringKey, params?: Record<string, string | number>): string {
  return fill(STRINGS[current][key], params);
}
