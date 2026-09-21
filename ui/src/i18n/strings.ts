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
  "qemu.missing": "QEMU (qemu-system-riscv64) was not found.",
  "qemu.install.windows":
    "On Windows, run `winget install SoftwareFreedomConservancy.QEMU` or install from https://www.qemu.org/download/#windows; you can also set the full path by hand.",
  "qemu.install.macos":
    "On macOS, run `brew install qemu` or install from https://www.qemu.org/download/; you can also set the full path by hand.",
  "qemu.install.linux":
    "On Linux, install your distribution's package (`qemu-system-misc` on Debian/Ubuntu, `qemu` on Arch/Fedora), or build from https://www.qemu.org/download/; you can also set the full path by hand.",
  "qemu.install.other":
    "Install QEMU from https://www.qemu.org/download/; you can also set the full path by hand.",
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
  "qemu.missing": "未找到 QEMU（qemu-system-riscv64）。",
  "qemu.install.windows":
    "Windows 可运行 `winget install SoftwareFreedomConservancy.QEMU`，或从 https://www.qemu.org/download/#windows 安装；也可手动指定完整路径。",
  "qemu.install.macos":
    "macOS 可运行 `brew install qemu`，或从 https://www.qemu.org/download/ 安装；也可手动指定完整路径。",
  "qemu.install.linux":
    "Linux 可用发行版包（Debian/Ubuntu 为 `qemu-system-misc`，Arch/Fedora 为 `qemu`），或从 https://www.qemu.org/download/ 自行构建；也可手动指定完整路径。",
  "qemu.install.other":
    "从 https://www.qemu.org/download/ 安装 QEMU；也可手动指定完整路径。",
};

/** The registry itself: language -> key -> template. */
export const STRINGS: Record<Language, Record<StringKey, string>> = { en: EN, zh: ZH };
