/**
 * Platform wording (v0.7 batch A).
 *
 * Dependency-free so `ui/scripts/probe-ui-i18n.mjs` can import the shipped rule:
 * which platform the webview is on, and which registry key carries that platform's
 * QEMU install hint. The strings themselves live in the i18n registry (`../i18n`),
 * so the hint reads in the user's language as well as their platform.
 */
import type { StringKey } from "../i18n/index.ts";

/** A platform whose install hint differs. */
export type Platform = "windows" | "macos" | "linux" | "other";

/**
 * Classify what `navigator.platform` / `navigator.userAgent` reports.
 *
 * macOS is checked first on purpose: a `userAgent` there says `Darwin`, which would
 * otherwise match the Windows test.
 */
export function parsePlatform(raw: unknown): Platform {
  const text = typeof raw === "string" ? raw.toLowerCase() : "";
  if (text.includes("mac") || text.includes("darwin")) return "macos";
  if (text.includes("linux")) return "linux";
  if (text.includes("win")) return "windows";
  return "other";
}

/** The platform as the webview reports it (`""` outside a browser). */
export function navigatorPlatform(): string {
  const nav = (globalThis as { navigator?: { platform?: unknown; userAgent?: unknown } }).navigator;
  if (typeof nav?.platform === "string" && nav.platform.length > 0) return nav.platform;
  return typeof nav?.userAgent === "string" ? nav.userAgent : "";
}

/** The registry key holding the QEMU install hint for `raw`'s platform. */
export function qemuInstallKey(raw: unknown): StringKey {
  switch (parsePlatform(raw)) {
    case "windows":
      return "qemu.install.windows";
    case "macos":
      return "qemu.install.macos";
    case "linux":
      return "qemu.install.linux";
    default:
      return "qemu.install.other";
  }
}
