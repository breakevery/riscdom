/**
 * The self-built i18n facility (v0.7 batches 1-2): the language the interface is
 * showing, the preference behind it, and the one place that turns a registry key
 * into text.
 *
 * Dependency-free on purpose, like `lib/theme.ts` and `lib/runView.ts`: the node
 * probes and `scripts/check-ui-strings.mjs` import the shipped module directly
 * (Node strips the types), so nothing here may pull in React or the DOM. The
 * preference's storage and the `lang` attribute live in `appStore` (a manual
 * choice gets the last word); this module holds what `t()` reads and the rules
 * that turn a preference into a language.
 */
import { LANGUAGES, STRINGS } from "./strings.ts";
import type { Language, StringKey } from "./strings.ts";

export { LANGUAGES } from "./strings.ts";
export type { Language, StringKey } from "./strings.ts";

/** The language used when the system says nothing we recognise. */
export const DEFAULT_LANGUAGE: Language = "en";

/** What the user chooses: follow the system, or pin one language (v0.7 batch 2). */
export type LanguageChoice = "system" | Language;

/** The choices the settings page offers, in order. */
export const LANGUAGE_CHOICES: readonly LanguageChoice[] = ["system", "en", "zh"];

/** Parse a stored preference; anything unknown means "follow the system". */
export function parseLanguageChoice(raw: unknown): LanguageChoice {
  return typeof raw === "string" && (LANGUAGES as readonly string[]).includes(raw)
    ? (raw as Language)
    : "system";
}

/** A BCP-47 tag ("zh-CN", "en-US", …) as one of the registry's languages. */
export function detectLanguage(tag: unknown): Language {
  return typeof tag === "string" && tag.toLowerCase().startsWith("zh")
    ? "zh"
    : DEFAULT_LANGUAGE;
}

/** What the system reports, or `undefined` outside a browser. */
export function navigatorLanguage(): unknown {
  return (globalThis as { navigator?: { language?: unknown } }).navigator?.language;
}

/** The language a preference means right now: `system` follows `tag`. */
export function resolveLanguage(choice: LanguageChoice, tag: unknown): Language {
  const parsed = parseLanguageChoice(choice);
  return parsed === "system" ? detectLanguage(tag) : parsed;
}

/** The `lang` attribute a language writes (`zh` becomes `zh-CN`). */
export function langAttribute(language: Language): string {
  return language === "zh" ? "zh-CN" : "en";
}

/** The registry key for a choice's label. */
export function languageChoiceKey(choice: LanguageChoice): StringKey {
  switch (parseLanguageChoice(choice)) {
    case "en":
      return "language.english";
    case "zh":
      return "language.chinese";
    default:
      return "language.system";
  }
}

let current: Language = resolveLanguage("system", navigatorLanguage());
const listeners = new Set<() => void>();

/** The language `t()` reads right now. */
export function getLanguage(): Language {
  return current;
}

/** Make `t()` read `next`; subscribers (the store) re-render on the change. */
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
