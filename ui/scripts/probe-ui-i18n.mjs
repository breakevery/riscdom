#!/usr/bin/env node
/**
 * i18n probe (v0.7 batches 1-2 — the language switch).
 *
 * Checks that the preference resolves the way the settings page promises (follow
 * the system, or pin one language), that it reaches the strings the UI renders,
 * that the store re-renders when it changes rather than leaving already-built
 * sentences behind, and that `lang` on <html> follows it.
 *
 * The rules live in `ui/src/i18n/index.ts` and `ui/src/i18n/strings.ts` —
 * dependency-free modules — so this imports the shipped code directly (Node
 * strips the types).
 *
 *   node ui/scripts/probe-ui-i18n.mjs
 */
import { readFileSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const MODULE = path.join(REPO, "ui", "src", "i18n", "index.ts");
const RUNS = path.join(REPO, "ui", "src", "lib", "runView.ts");
const TAB = path.join(REPO, "ui", "src", "settings", "AppearanceTab.tsx");
const AUDIT = path.join(REPO, "ui", "src", "settings", "AuditTab.tsx");
const TOOLCHAIN = path.join(REPO, "ui", "src", "settings", "ToolchainTab.tsx");
const PLATFORM = path.join(REPO, "ui", "src", "lib", "platform.ts");
const STORE = path.join(REPO, "ui", "src", "state", "appStore.ts");
const API = path.join(REPO, "ui", "src", "api", "tauri.ts");
const SHELL = path.join(REPO, "ui", "src-tauri", "src", "lib.rs");
const HOST_CMDS = path.join(REPO, "host-tauri", "src", "commands.rs");

let failures = 0;
function check(name, ok, detail) {
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `: ${detail}` : ""}`);
}

const i18n = await import(pathToFileURL(MODULE).href);
const runs = await import(pathToFileURL(RUNS).href);
const platform = await import(pathToFileURL(PLATFORM).href);
console.log(`# language rules (${path.relative(REPO, MODULE)})\n`);

check("the choice is three-state", i18n.LANGUAGE_CHOICES.join(",") === "system,en,zh");
check(
  "junk reads as follow-the-system",
  i18n.parseLanguageChoice("fr") === "system" &&
    i18n.parseLanguageChoice(null) === "system" &&
    i18n.parseLanguageChoice("zh") === "zh",
);
check("system follows a Chinese OS", i18n.resolveLanguage("system", "zh-CN") === "zh");
check("system follows an English OS", i18n.resolveLanguage("system", "en-US") === "en");
check(
  "an explicit choice ignores the OS",
  i18n.resolveLanguage("zh", "en-US") === "zh" &&
    i18n.resolveLanguage("en", "zh-CN") === "en",
);
check(
  "the lang attribute is the BCP-47 form",
  i18n.langAttribute("zh") === "zh-CN" && i18n.langAttribute("en") === "en",
);
check(
  "each choice has a registry label",
  i18n.languageChoiceKey("system") === "language.system" &&
    i18n.languageChoiceKey("en") === "language.english" &&
    i18n.languageChoiceKey("zh") === "language.chinese",
);

// Switching the language changes what the UI renders — including a sentence the
// store used to cache (`diffError` now keeps the raw reason).
const diffRows = [
  { field: "llm", a: { model: "a" }, b: { model: "b" }, is_different: true },
];
i18n.setLanguage("en");
const headlineEn = runs.diffTitle(diffRows);
const systemLabelEn = i18n.t("language.system");
i18n.setLanguage("zh");
const headlineZh = runs.diffTitle(diffRows);
const systemLabelZh = i18n.t("language.system");
check(
  "switching the language switches the diff headline",
  headlineEn !== headlineZh &&
    headlineZh === i18n.t("diff.headline", { fields: 1, different: 1 }),
  `${headlineEn} / ${headlineZh}`,
);
check(
  "switching the language switches the chooser's own labels",
  systemLabelEn !== systemLabelZh,
  `${systemLabelEn} / ${systemLabelZh}`,
);
check(
  "the refusal sentence is built from the raw reason",
  runs.diffFailedText("boom") === i18n.t("diff.failed", { reason: "boom" }),
  runs.diffFailedText("boom"),
);
i18n.setLanguage("en");

// Wiring: the tab, the store, the shell and the host all agree.
const tab = readFileSync(TAB, "utf8");
check("the settings page offers the language choices", /LANGUAGE_CHOICES\.map/.test(tab));
check("the settings page picks a language", /store\.setLanguage\(choice\)/.test(tab));
check("the settings page marks the active choice", /store\.languageChoice === choice/.test(tab));
check("the settings page labels come from the registry", /t\(languageChoiceKey\(choice\)\)/.test(tab));

const store = readFileSync(STORE, "utf8");
check("the store loads the stored choice", /api\.getLanguage\(\)/.test(store));
check("the store persists a choice", /api\.setLanguage\(next\)/.test(store));
check("the store writes the lang attribute", /document\.documentElement\.lang = langAttribute\(/.test(store));
check("the store follows the OS while on `system`", /languagechange/.test(store));
check(
  "the store re-renders when the language changes",
  /useSyncExternalStore\(subscribeLanguage, getLanguage\)/.test(store),
);
check(
  "the store keeps the host's refusal verbatim",
  /diffError: string \| null/.test(store) && !/diffFailedText\(/.test(store),
  "appStore must not build the refused sentence",
);
check(
  "the panel builds the refusal at render time",
  /diffFailedText\(store\.diffError\)/.test(readFileSync(AUDIT, "utf8")),
);

// Platform wording (v0.7 batch A): the QEMU "not found" banner follows the platform
// and reads from the registry, instead of hard-coding the Windows route.
check(
  "the platform classifies what the webview reports",
  platform.parsePlatform("Win32") === "windows" &&
    platform.parsePlatform("MacIntel") === "macos" &&
    platform.parsePlatform("Linux x86_64") === "linux" &&
    platform.parsePlatform("Darwin") === "macos" &&
    platform.parsePlatform(undefined) === "other",
);
check(
  "each platform picks its own install key",
  platform.qemuInstallKey("Win32") === "qemu.install.windows" &&
    platform.qemuInstallKey("MacIntel") === "qemu.install.macos" &&
    platform.qemuInstallKey("Linux x86_64") === "qemu.install.linux" &&
    platform.qemuInstallKey("FreeBSD") === "qemu.install.other",
);
const installKeys = [
  "qemu.missing",
  "qemu.install.windows",
  "qemu.install.macos",
  "qemu.install.linux",
  "qemu.install.other",
];
i18n.setLanguage("en");
const installEn = installKeys.map((key) => i18n.t(key));
i18n.setLanguage("zh");
const installZh = installKeys.map((key) => i18n.t(key));
i18n.setLanguage("en");
check(
  "every install hint has text in both languages",
  installKeys.every(
    (key, index) =>
      installEn[index].length > 0 &&
      installZh[index].length > 0 &&
      installEn[index] !== key &&
      installZh[index] !== key,
  ),
  `${installEn[0]} / ${installZh[0]}`,
);
const toolchain = readFileSync(TOOLCHAIN, "utf8");
check(
  "the QEMU banner follows the platform and the registry",
  /t\(qemuInstallKey\(navigatorPlatform\(\)\)\)/.test(toolchain) &&
    !/winget install/.test(toolchain),
  "ToolchainTab must not hard-code the Windows route",
);

const api = readFileSync(API, "utf8");
const shell = readFileSync(SHELL, "utf8");
const cmds = readFileSync(HOST_CMDS, "utf8");
for (const command of ["get_language", "set_language"]) {
  check(`the wrapper calls \`${command}\``, api.includes(`"${command}"`));
  check(`\`${command}\` is registered in the shell`, shell.includes(`commands::${command}`));
  check(`\`${command}\` exists in the host`, cmds.includes(`pub async fn ${command}`));
}

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
