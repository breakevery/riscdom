/**
 * UI string registry (v0.7 batch 1).
 *
 * The self-built i18n facility's data: one table per language, the same keys in
 * each, so a missing or empty translation is visible in this file itself. English
 * declares the key set (and is the fallback); its wording for the v0.6 diff block
 * mirrors the table `ui/README.md` kept "so it can be mirrored when the interface
 * becomes translatable".
 *
 * Dependency-free on purpose, like `lib/theme.ts`: `ui/scripts/probe-ui-runs.mjs`
 * and `scripts/check-ui-strings.mjs` import this module directly (Node strips the
 * types), so it may not reach for React or the DOM.
 */

/** A language the interface can be shown in. */
export type Language = "en" | "zh";

/** Every language the registry carries, the fallback first. */
export const LANGUAGES: readonly Language[] = ["en", "zh"];

/** English defines the key set; every other language must match it exactly. */
const EN = {
  "diff.headline": "field-level differences · {fields} fields · {different} differ",
  "diff.loading": "reading the differences…",
  "diff.failed": "could not read the fingerprint diff: {reason}",
  "diff.empty": "these two runs' fingerprints share no field to compare.",
  "language.heading": "Language",
  "language.system": "Follow the system",
  "language.english": "English",
  "language.chinese": "中文",
} as const;

/** A key of the registry. */
export type StringKey = keyof typeof EN;

/** The other language: every key of `EN`, and nothing else. */
const ZH: Record<StringKey, string> = {
  "diff.headline": "字段级差异 · {fields} 个字段 · {different} 处不同",
  "diff.loading": "差异读取中…",
  "diff.failed": "指纹差异读取失败：{reason}",
  "diff.empty": "这两个 run 的指纹里没有可比字段。",
  "language.heading": "语言",
  "language.system": "跟随系统",
  "language.english": "English",
  "language.chinese": "中文",
};

/** The registry itself: language -> key -> template. */
export const STRINGS: Record<Language, Record<StringKey, string>> = { en: EN, zh: ZH };
