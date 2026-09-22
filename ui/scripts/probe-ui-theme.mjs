#!/usr/bin/env node
/**
 * Theme probe (v0.4 batch 6 — #11a theme switching).
 *
 * Checks the three-state cycle and what "follow the system" resolves to, that the
 * stylesheet really defines every token the panels use, and that the choice is
 * wired to the host (so it survives a restart).
 *
 * The rules live in `ui/src/lib/theme.ts` — a dependency-free module — so this
 * imports the shipped code directly (Node strips the types).
 *
 *   node ui/scripts/probe-ui-theme.mjs
 */
import { readFileSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const MODULE = path.join(REPO, "ui", "src", "lib", "theme.ts");
const I18N = path.join(REPO, "ui", "src", "i18n", "index.ts");
const TAB = path.join(REPO, "ui", "src", "settings", "AppearanceTab.tsx");
const TABS = path.join(REPO, "ui", "src", "settings", "SettingsTabs.tsx");
const STORE = path.join(REPO, "ui", "src", "state", "appStore.ts");
const CANVAS = path.join(REPO, "ui", "src", "panels", "CanvasPanel.tsx");
const API = path.join(REPO, "ui", "src", "api", "tauri.ts");
const SHELL = path.join(REPO, "ui", "src-tauri", "src", "lib.rs");
const HOST_CMDS = path.join(REPO, "host-tauri", "src", "commands.rs");
const BASE_CSS = path.join(REPO, "ui", "src", "styles.css");
const PANELS_CSS = path.join(REPO, "ui", "src", "styles", "panels.css");
const LAYOUT_CSS = path.join(REPO, "ui", "src", "styles", "layout.css");

let failures = 0;
function check(name, ok, detail) {
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `: ${detail}` : ""}`);
}

const theme = await import(pathToFileURL(MODULE).href);
const i18n = await import(pathToFileURL(I18N).href);
// The Chinese assertions below are the labels the app ships in its other language;
// the registry's default is English, so pin it (v0.8 batch 2, when these strings
// moved into the registry).
i18n.setLanguage("zh");
console.log(`# theme rules (${path.relative(REPO, MODULE)})\n`);

check("the cycle is three-state", theme.THEMES.join(",") === "light,dark,system");
check("light → dark", theme.nextTheme("light") === "dark");
check("dark → system", theme.nextTheme("dark") === "system");
check("system → light", theme.nextTheme("system") === "light");
check("junk reads as system", theme.parseTheme("neon") === "system" && theme.parseTheme(null) === "system");
check("labels are human", theme.themeLabel("light") === "浅色" && theme.themeLabel("dark") === "深色" && theme.themeLabel("system") === "跟随系统");

check("system follows a dark OS", theme.resolveTheme("system", true) === "dark");
check("system follows a light OS", theme.resolveTheme("system", false) === "light");
check("an explicit choice ignores the OS", theme.resolveTheme("light", true) === "light" && theme.resolveTheme("dark", false) === "dark");

const attributes = {};
const fakeRoot = {
  setAttribute(name, value) {
    attributes[name] = value;
  },
};
check("applying writes data-theme", theme.applyTheme("light", fakeRoot, true) === "light" && attributes["data-theme"] === "light");
check("applying system writes what it resolved to", theme.applyTheme("system", fakeRoot, false) === "light" && attributes["data-theme"] === "light");

check("the summary names the resolved theme", theme.themeSummary("system", "dark").includes("跟随系统") && theme.themeSummary("system", "dark").includes("深色"));
check("the summary names a fixed choice", theme.themeSummary("light", "light").includes("固定为浅色"));
check("an unknown media query stays dark", theme.systemPrefersDark({}) === true);

// The stylesheet must define every token the panels reference: a typo would
// silently drop the colour.
const base = readFileSync(BASE_CSS, "utf8");
const panels = readFileSync(PANELS_CSS, "utf8");
const layout = readFileSync(LAYOUT_CSS, "utf8");
const defined = new Set([...base.matchAll(/^\s*(--[a-z0-9-]+)\s*:/gm)].map((m) => m[1]));
const used = new Set([...(panels + layout + base).matchAll(/var\((--[a-z0-9-]+)/g)].map((m) => m[1]));
const missing = [...used].filter((token) => !defined.has(token));
check("every token used is defined", missing.length === 0, missing.join(","));
check("both palettes exist", /\[data-theme="light"\]/.test(base) && /color-scheme:\s*dark/.test(base) && /color-scheme:\s*light/.test(base));
// Comments are stripped first, so a ticket number like `#11a` is not a colour.
const stripComments = (css) => css.replace(/\/\*[\s\S]*?\*\//g, "");
check(
  "no raw colours outside the palettes",
  !/#[0-9a-fA-F]{3,6}/.test(stripComments(panels)),
  "panels.css should use tokens only",
);
check("the light palette overrides the core tokens", ["--bg", "--fg", "--accent", "--panel", "--border"].every((token) => new RegExp(`${token}\\s*:`).test(base.split('[data-theme="light"]')[1] ?? "")));

// Wiring: the tab, the store, the terminal and the host all agree.
const tab = readFileSync(TAB, "utf8");
check("the tab offers every choice", /THEMES\.map/.test(tab) && /themeLabel\(candidate\)/.test(tab));
check("the tab can cycle", /store\.cycleTheme\(\)/.test(tab));
check("the tab shows what the choice means", /themeSummary\(store\.theme, store\.resolvedTheme\)/.test(tab));
check("the tab is registered", /"appearance"/.test(readFileSync(TABS, "utf8")));

const store = readFileSync(STORE, "utf8");
check("the store loads the stored choice", /api\.getTheme\(\)/.test(store));
check("the store persists a choice", /api\.setTheme\(next\)/.test(store));
check("the store applies it to the document", /applyTheme\(theme, document\.documentElement/.test(store));
check("the store follows the OS while on `system`", /prefers-color-scheme: dark/.test(store));
check("the terminal follows the theme", /terminalTheme\(\)/.test(readFileSync(CANVAS, "utf8")));

const api = readFileSync(API, "utf8");
const shell = readFileSync(SHELL, "utf8");
const cmds = readFileSync(HOST_CMDS, "utf8");
for (const command of ["get_theme", "set_theme"]) {
  check(`the wrapper calls \`${command}\``, api.includes(`"${command}"`));
  check(`\`${command}\` is registered in the shell`, shell.includes(`commands::${command}`));
  check(`\`${command}\` exists in the host`, cmds.includes(`pub async fn ${command}`));
}

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
