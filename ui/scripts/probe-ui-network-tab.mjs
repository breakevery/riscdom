#!/usr/bin/env node
/**
 * The network face (v0.9.9 内网接入) — batch 2, the configuration skeleton.
 *
 * What this probe can see without a desktop: the wiring exists and is wired the
 * way the batch decided. **What it cannot see** is the page working — probes run
 * in Node, where `isTauriRuntime()` is false and no React tree is rendered, so the
 * network tab is exactly the kind of screen that has to be walked by hand (see
 * `docs/manual-acceptance.md`, layer 2). Three things are worth pinning anyway,
 * because each of them fails quietly:
 *
 * - **the browser never gets the tab**, and the three names reject instead of
 *   pretending (`NetworkTab` is wrapped, `SettingsTabs` filters, `http.ts` says so);
 * - **the token is read, never created**: the command must not reach for
 *   `load_or_create`, which would mint a credential merely because a settings page
 *   was opened, and the file name it reads has to be the server's own constant;
 * - **settings.json gains the field additively** — the same five fields on both
 *   sides of the boundary, and no `SETTINGS_VERSION` move.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const SRC = path.join(REPO, "ui", "src");

const TAB = path.join(SRC, "settings", "NetworkTab.tsx");
const TABS = path.join(SRC, "settings", "SettingsTabs.tsx");
const STORE = path.join(SRC, "state", "appStore.ts");
const TYPES = path.join(SRC, "api", "types.ts");
const TAURI = path.join(SRC, "api", "tauri.ts");
const HTTP = path.join(SRC, "api", "http.ts");
const SHELL = path.join(REPO, "ui", "src-tauri", "src", "lib.rs");
const SETTINGS = path.join(REPO, "host-core", "src", "settings.rs");
const SERVER_TOKEN = path.join(REPO, "server", "src", "token.rs");
const STRINGS = path.join(SRC, "i18n", "strings.ts");
const GATE = path.join(REPO, "scripts", "gate.sh");

let failures = 0;

function check(name, ok, detail) {
  if (ok) {
    console.log(`PASS  ${name}${detail ? `: ${detail}` : ""}`);
  } else {
    failures += 1;
    console.log(`FAIL  ${name}${detail ? `: ${detail}` : ""}`);
  }
}

const read = (p) => readFileSync(p, "utf8");
/** The file's code without comments: a word mentioned in a doc comment is not code. */
const code = (s) => s.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");

// ----- the page ---------------------------------------------------------------

const tab = code(read(TAB));
check(
  "the network tab exists and takes the store",
  /export default function NetworkTab\(\{ store \}: \{ store: AppStore \}\)/.test(tab),
);
check(
  "it acts through the store's three actions",
  /store\.refreshNetwork\(\)/.test(tab) &&
    /store\.setNetwork\(/.test(tab) &&
    /store\.readLanToken\(\)/.test(tab),
);
check(
  "the token is shown only when it is asked for",
  /useState<string \| null>\(null\)/.test(tab) &&
    /onClick=\{\(\) => void showToken\(\)\}/.test(tab),
);
check(
  "the connect button is disabled: the next batches implement it",
  /className="primary" disabled/.test(tab),
);
check(
  "the allow-LAN switch carries its warning",
  /network\.allow_lan_warning/.test(tab) && /role="alert"/.test(tab),
);

// ----- the page is the desktop's ----------------------------------------------

const tabs = code(read(TABS));
check(
  "the settings page offers the tab",
  /id: "network"/.test(tabs) && /settings\.tab\.network/.test(tabs),
);
check(
  "the browser is not offered it",
  /entry\.id !== "network"/.test(tabs) &&
    /TabId =[\s\S]{0,220}"network"/.test(tabs.replace(/\s+/g, " ")),
);
check(
  "its body is wrapped like the model form",
  (tabs.match(/<DesktopOnly>/g) ?? []).length === 2,
);

// ----- the store --------------------------------------------------------------

const store = code(read(STORE));
check(
  "the store declares the state and the three actions",
  /network: api\.NetworkSettings \| null;/.test(store) &&
    /refreshNetwork: \(\) => Promise<void>;/.test(store) &&
    /setNetwork: \(next: Partial<api\.NetworkSettings>\) => Promise<void>;/.test(store) &&
    /readLanToken: \(\) => Promise<string>;/.test(store),
);
check(
  "the store implements them",
  /const refreshNetwork = useCallback/.test(store) &&
    /const setNetwork = useCallback/.test(store) &&
    /const readLanToken = useCallback/.test(store),
);
check(
  "the store hands them out",
  ["network,", "refreshNetwork,", "setNetwork,", "readLanToken,"].every((name) =>
    new RegExp(`^\\s{4}${name}$`, "m").test(store),
  ),
);

// ----- the shape, on both sides -----------------------------------------------

const FIELDS = ["remote_url", "remote_token", "lan_enabled", "lan_bind", "lan_allow_lan"];
const types = read(TYPES);
check(
  "the browser's shape carries the five fields",
  FIELDS.every((field) => types.includes(`${field}:`)),
  FIELDS.join(", "),
);
const settings = read(SETTINGS);
check(
  "the host's struct carries the same five",
  FIELDS.every((field) => settings.includes(`pub ${field}:`)),
  FIELDS.join(", "),
);
check(
  "settings.json gains it additively",
  /pub network: Option<NetworkSettings>/.test(settings) &&
    /pub const SETTINGS_VERSION: u32 = 1;/.test(settings),
);

// ----- two transports, held to each other -------------------------------------

const tauri = code(read(TAURI));
check(
  "the desktop implementation calls the shell's commands",
  /invoke<NetworkSettings \| null>\("get_network"\)/.test(tauri) &&
    /invoke<void>\("set_network"/.test(tauri) &&
    /invoke<string>\("read_lan_token"\)/.test(tauri),
);
const http = code(read(HTTP));
check(
  "the Web implementation rejects with a sentence",
  /export const readLanToken = \(\): Promise<string> => desktopNetwork\("read_lan_token"\)/.test(
    http,
  ) && /desktopNetwork\("get_network"\)/.test(http) && /desktopNetwork\("set_network"\)/.test(http),
);

// ----- the shell's three commands ---------------------------------------------

const shell = code(read(SHELL));
check(
  "the shell defines and registers three commands",
  ["fn get_network(", "fn set_network(", "fn read_lan_token("].every((f) => shell.includes(f)) &&
    ["get_network,", "set_network,", "read_lan_token,"].every((n) => shell.includes(n)),
);
check(
  "the token is read, never created",
  !/load_or_create/.test(shell) && /std::fs::read_to_string/.test(shell),
);
check(
  "...and the file it reads is the server's own name",
  /pub const TOKEN_FILE: &str = "token";/.test(read(SERVER_TOKEN)) &&
    /join\("token"\)/.test(shell),
);

// ----- the words --------------------------------------------------------------

const KEYS = [
  "settings.tab.network",
  "network.out_heading",
  "network.out_hint",
  "network.remote_url",
  "network.remote_token",
  "network.connect",
  "network.in_heading",
  "network.lan_enabled",
  "network.lan_bind",
  "network.lan_bind_hint",
  "network.allow_lan",
  "network.allow_lan_warning",
  "network.lan_url",
  "network.lan_url_pending",
  "network.lan_state_running",
  "network.firewall_hint",
  "network.token_show",
  "network.token_copy",
  "network.token_path",
  "network.save",
  "network.saved",
];
const registry = await import(pathToFileURL(STRINGS).href);
const missing = KEYS.filter((key) => {
  const en = registry.STRINGS.en[key];
  const zh = registry.STRINGS.zh[key];
  return typeof en !== "string" || typeof zh !== "string" || en === "" || zh === "";
});
check(
  "every network key exists in both languages",
  missing.length === 0,
  missing.join(", ") || `${KEYS.length} keys x 2 languages`,
);

const used = [...new Set([...code(read(TAB)).matchAll(/\bt\(\s*"([a-z0-9_.]+)"/g)].map((m) => m[1]))];
const undeclared = used.filter((key) => !KEYS.includes(key));
check(
  "the page asks for nothing outside the declared set",
  undeclared.length === 0,
  undeclared.join(", ") || `${used.length} of ${KEYS.length} keys used`,
);

check("gate.sh runs this probe", /probe-ui-network-tab\.mjs/.test(read(GATE)));

if (failures > 0) {
  console.log(`\n${failures} failing check(s)`);
  process.exit(1);
}
console.log("\nOK");
