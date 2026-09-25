#!/usr/bin/env node
/**
 * The remote face (v0.9.9 内网接入 4/N) — a desktop connected to an in-network node.
 *
 * The batch's whole risk is in one sentence: **the same window can now be looking
 * at a different machine's node**, and almost every rule this codebase keeps is
 * phrased in terms of "the desktop" or "the browser" — which is no longer the same
 * question as "this host" or "another host". So this probe pins the three things
 * that make the new mode safe to add:
 *
 * - **the mode is a value, not the environment**: `api/index.ts` holds it in a
 *   variable, every data-plane name forwards through it, and the eight names that
 *   *wire a node* deliberately do not — they act on the machine the window runs on
 *   in every mode, which is the only reason the way back can work;
 * - **the credential is not in a settings file**: `NetworkSettings` has no token
 *   field, and the token lives in the OS keyring under `remote-token:<host>`;
 * - **each screen knows which host it is looking at**: the gate asks `isLocalHost`,
 *   the settings page filters by mode, and the top bar says which node is on screen.
 *
 * Nothing here renders React: probes run in Node, where `isTauriRuntime()` is false.
 * What cannot be checked here is the mode actually switching — that is the manual
 * walk in `docs/manual-acceptance.md` layer 9.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const SRC = path.join(REPO, "ui", "src");

const INDEX = path.join(SRC, "api", "index.ts");
const APP = path.join(SRC, "App.tsx");
const LOGIN = path.join(SRC, "Login.tsx");
const SHELL = path.join(SRC, "layout", "AppShell.tsx");
const TABS = path.join(SRC, "settings", "SettingsTabs.tsx");
const STORE = path.join(SRC, "state", "appStore.ts");
const STRINGS = path.join(SRC, "i18n", "strings.ts");
const RUST_SHELL = path.join(REPO, "ui", "src-tauri", "src", "lib.rs");
const SETTINGS = path.join(REPO, "host-core", "src", "settings.rs");
const KEYRING = path.join(REPO, "host-core", "src", "keyring.rs");
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

const index = read(INDEX);
const indexCode = code(index);
const app = code(read(APP));
const login = code(read(LOGIN));
const shell = code(read(SHELL));
const tabs = read(TABS);
const store = code(read(STORE));
const rustShell = code(read(RUST_SHELL));

// ----- the mode lives in the adapter ------------------------------------------

check(
  "the mode is a value the adapter holds, not the environment",
  /let current: SharedApi = isTauriRuntime\(\) \? tauri : http;/.test(indexCode) &&
    /export function setImpl\(next: Mode, url = ""\): void \{/.test(indexCode) &&
    /http\.setApiBase\(url\);/.test(indexCode),
);
check(
  "the two predicates answer different questions",
  /export function isRemote\(\): boolean \{\s*return mode === "remote";/.test(indexCode) &&
    /export function isLocalHost\(\): boolean \{\s*return isTauriRuntime\(\) && mode !== "remote";/.test(
      indexCode,
    ),
);
check(
  "the environment predicate is untouched",
  /export function isTauriRuntime\(\): boolean \{/.test(indexCode),
);

// The data plane forwards; the node's own wiring does not.

const forwarders = [...indexCode.matchAll(/^export const (\w+): SharedApi\["(\w+)"\] = /gm)];
const mismatched = forwarders.filter(([, name, typed]) => name !== typed);
check(
  "every forwarder is typed by the name it forwards",
  forwarders.length > 0 && mismatched.length === 0,
  `${forwarders.length} forwarders`,
);

const viaCurrent = indexCode.match(/current\.\w+\(\.\.\.args\);/g) ?? [];
const viaRuntime = indexCode.match(/isTauriRuntime\(\) \? tauri\.\w+\(\.\.\.args\) : http\.\w+\(\.\.\.args\);/g) ?? [];
check(
  "the data plane goes through whichever implementation is live",
  viaCurrent.length === 60 && forwarders.length === 68,
  `${viaCurrent.length} of ${forwarders.length} forwarders`,
);

const LOCAL_ALWAYS = [
  "getNetwork",
  "setNetwork",
  "readLanToken",
  "lanStatus",
  "saveRemoteToken",
  "readRemoteToken",
  "clearRemoteToken",
  "restartApp",
];
check(
  "eight names wire this machine in every mode",
  viaRuntime.length === LOCAL_ALWAYS.length &&
    LOCAL_ALWAYS.every((name) => indexCode.includes(`tauri.${name}(...args) : http.${name}(...args);`)),
  `${viaRuntime.length} of them`,
);
const leaked = LOCAL_ALWAYS.filter((name) => indexCode.includes(`current.${name}(...args)`));
check(
  "...and none of them follows the mode",
  leaked.length === 0,
  leaked.join(", ") || "remote mode cannot lose the way back",
);

// ----- the gate ---------------------------------------------------------------

check(
  "the gate is settled by the mode, and read before the shell mounts",
  /api\.getNetwork\(\)/.test(app) &&
    /api\.setApiBase\(url\);/.test(app) &&
    /api\.setImpl\("remote", url\);/.test(app) &&
    /api\.readRemoteToken\(url\)/.test(app) &&
    /api\.setToken\(token, false\);/.test(app) &&
    /if \(api\.isLocalHost\(\)\) return <AppShell \/>;/.test(app),
);
check(
  "a window with nothing to settle does not wait",
  /useState\(\(\) => !api\.isTauriRuntime\(\)\)/.test(app),
);
check(
  "the way back deletes the token, clears the address, and restarts",
  /api\.clearRemoteToken\(url\)/.test(app) &&
    /remote_url: null,/.test(app) &&
    /api\.restartApp\(\)/.test(app),
);
check(
  "...and is offered only by a runtime that can act on this machine",
  /onUseLocal=\{api\.isTauriRuntime\(\) \? \(\) => void useLocal\(\) : undefined\}/.test(app) &&
    /onUseLocal\?: \(\) => void;/.test(login),
);
check(
  "a failed write does not restart into the same state",
  /setLeaveProblem\(String\(e\)\);\s*setLeaving\(false\);\s*return;/.test(app),
);

// ----- the screens know which host they show ----------------------------------

check(
  "the settings page filters by mode, not by runtime",
  /const localHost = isLocalHost\(\);\s*const keep = localHost \? null : isRemote\(\) \? REMOTE_TABS : WEB_TABS;/.test(
    code(tabs),
  ) &&
    /const REMOTE_TABS: TabId\[\] = \["audit", "appearance", "network"\];/.test(tabs) &&
    /const WEB_TABS: TabId\[\] = \["audit", "appearance"\];/.test(tabs),
);
check(
  "the top bar says which node is on screen, and never the token",
  /const remote = isRemote\(\)/.test(shell) &&
    /t\("app\.remote_badge"\)/.test(shell) &&
    /t\("app\.remote_badge_title", \{ host: getApiBase\(\) \}\)/.test(shell) &&
    !/remote_token/.test(shell),
);
check(
  "the status page follows the host, not the runtime",
  /const webClient = !isLocalHost\(\)/.test(shell),
);
check(
  "the remote window takes the event stream like the browser does",
  /if \(api\.isLocalHost\(\)\) return;/.test(store),
);

// ----- the credential is not a setting ----------------------------------------

const settings = read(SETTINGS);
check(
  "the settings struct has no token field",
  /pub struct NetworkSettings \{/.test(settings) &&
    !/pub remote_token:/.test(settings) &&
    /pub remote_url: Option<String>/.test(settings),
);
check(
  "...and says where it went instead",
  /OS keyring under `remote-token:<host>`/.test(settings),
);
check(
  "the keyring names it beside the provider key",
  /pub fn user_for_remote\(host: &str\) -> String \{\s*format!\("remote-token:\{host\}"\)/.test(
    read(KEYRING),
  ),
);
check(
  "the shell's four commands use it and are registered",
  ["fn save_remote_token(", "fn read_remote_token(", "fn clear_remote_token(", "fn restart_app("].every(
    (f) => rustShell.includes(f),
  ) &&
    [
      "save_remote_token,",
      "read_remote_token,",
      "clear_remote_token,",
      "restart_app,",
    ].every((n) => rustShell.includes(n)) &&
    /host_tauri::keyring::user_for_remote\(host\)/.test(rustShell),
);
check(
  "the restart is Tauri's own, with no plugin added",
  /app\.restart\(\)/.test(rustShell) && !/tauri-plugin-process/.test(rustShell),
);
check(
  "the keyring is read through the app's handle, not a copy of it",
  /state\s*\.keyring\s*\.set\(/.test(rustShell.replace(/\s+/g, " ")) ||
    /state\.keyring\.set\(/.test(rustShell),
);

// ----- the words ---------------------------------------------------------------

const KEYS = [
  "app.booting",
  "app.remote_badge",
  "app.remote_badge_title",
  "login.use_local",
  "login.use_local_hint",
];
const registry = await import(pathToFileURL(STRINGS).href);
const missing = KEYS.filter((key) => {
  const en = registry.STRINGS.en?.[key];
  const zh = registry.STRINGS.zh?.[key];
  return typeof en !== "string" || typeof zh !== "string" || en === "" || zh === "";
});
check(
  "every key this face adds exists in both languages",
  missing.length === 0,
  missing.join(", ") || `${KEYS.length} keys x 2 languages`,
);

check("gate.sh runs this probe", /probe-ui-remote\.mjs/.test(read(GATE)));

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
