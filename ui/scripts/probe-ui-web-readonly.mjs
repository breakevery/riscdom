#!/usr/bin/env node
/**
 * The read-only board probe (v0.9 D2b-4a).
 *
 * The browser must show the node, not change it — and "not change it" is enforced one
 * way only: every control is wrapped in `DesktopOnly`, which renders nothing when
 * `isTauriRuntime()` is false. That is easy to get wrong by *forgetting a wrap*, and a
 * forgotten wrap is invisible in review (the desktop still looks right, and the browser
 * simply offers a button that fails). So this probe counts the wraps per screen, checks
 * that the one screen allowed to change something is not wrapped, and checks the two
 * controls the browser *does* implement.
 *
 * `components/DesktopOnly.tsx` is JSX, so it is read as source (the same way the other
 * probes read the sources they cannot import) while `strings.ts` is imported.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const SRC = path.join(REPO, "ui", "src");

const COMPONENT = path.join(SRC, "components", "DesktopOnly.tsx");
const STRINGS = path.join(SRC, "i18n", "strings.ts");
const HTTP = path.join(SRC, "api", "http.ts");

/** Where a wrap count is expected. `DesktopOnly` tags, not controls: one wrap may
 * cover a group of buttons (the download row is two). */
const WRAPS = [
  ["settings/AuditTab.tsx", 2],
  ["settings/ToolchainTab.tsx", 5],
  ["settings/SnapshotTab.tsx", 2],
  ["settings/SettingsTabs.tsx", 2],
  ["panels/ChatPanel.tsx", 1],
  ["panels/CanvasPanel.tsx", 1],
];

let failures = 0;

function check(name, ok, detail) {
  if (ok) {
    console.log(`PASS  ${name}${detail ? `: ${detail}` : ""}`);
  } else {
    failures += 1;
    console.log(`FAIL  ${name}${detail ? `: ${detail}` : ""}`);
  }
}

const read = (relative) => readFileSync(path.join(SRC, relative), "utf8");
const count = (source, needle) => source.split(needle).length - 1;

/** The file's code without comments: a word mentioned in a doc comment is not code. */
const code = (source) => source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");

// ----- the mechanism ----------------------------------------------------------

const component = code(readFileSync(COMPONENT, "utf8"));
check(
  "DesktopOnly returns its children only in the desktop shell",
  /DesktopOnly[\s\S]*?isTauriRuntime\(\) \? <>\{children\}<\/> : null/.test(component),
);
check(
  "WebOnly is the exact mirror",
  /WebOnly[\s\S]*?isTauriRuntime\(\) \? null : <>\{children\}<\/>/.test(component),
);
check(
  "the mechanism asks the adapter, not a prop",
  /from "\.\.\/api"/.test(component) && !/readOnly/.test(component),
);

// ----- every control is wrapped, per screen -----------------------------------

for (const [file, expected] of WRAPS) {
  const source = read(file);
  const got = count(source, "<DesktopOnly>");
  check(`${file}: ${expected} DesktopOnly wrap(s)`, got === expected, `found ${got}`);
}

const appearance = read("settings/AppearanceTab.tsx");
check(
  "AppearanceTab is not wrapped: it is the one screen the browser may use",
  count(appearance, "<DesktopOnly>") === 0 && count(appearance, "<WebOnly>") === 1,
);

// ----- the browser is told why ------------------------------------------------

for (const file of [
  "settings/AuditTab.tsx",
  "settings/ToolchainTab.tsx",
  "settings/SnapshotTab.tsx",
  "settings/AppearanceTab.tsx",
]) {
  check(`${file} says why a control is missing`, /web\.readonly_note|web\.appearance_note/.test(read(file)));
}

// ----- the model form is not offered ------------------------------------------

const tabs = read("settings/SettingsTabs.tsx");
check(
  "SettingsTabs gives the browser the two tabs it can use",
  /const WEB_TABS: TabId\[\] = \["audit", "appearance"\]/.test(tabs) &&
    /isLocalHost\(\)/.test(tabs),
);
check(
  "…and a remote window the network face too: that page is its way back",
  /const REMOTE_TABS: TabId\[\] = \["audit", "appearance", "network"\]/.test(tabs) &&
    /isRemote\(\)/.test(tabs),
);
check(
  "…and a window never opens on a tab it does not have",
  /useState<TabId>\(keep === null \? "model" : keep\[0\]\)/.test(tabs),
);

// ----- the two controls the browser does implement ----------------------------

const http = readFileSync(HTTP, "utf8");
check(
  "setTheme is a real call, not a desktop-only refusal",
  /setTheme[\s\S]{0,200}POST[\s\S]{0,40}\/v0\/settings\/theme/.test(http) &&
    !/desktopOnly\("set_theme"\)/.test(http),
);
check(
  "setLanguage is a real call too",
  /setLanguage[\s\S]{0,200}POST[\s\S]{0,40}\/v0\/settings\/language/.test(http) &&
    !/desktopOnly\("set_language"\)/.test(http),
);

// ----- the wording exists in both languages -----------------------------------

const registry = await import(pathToFileURL(STRINGS).href);
for (const key of ["web.readonly_note", "web.appearance_note"]) {
  const en = registry.STRINGS.en?.[key];
  const zh = registry.STRINGS.zh?.[key];
  check(
    `${key} exists in both languages`,
    typeof en === "string" && en.trim() !== "" && typeof zh === "string" && zh.trim() !== "",
  );
}

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
