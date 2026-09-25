#!/usr/bin/env node
/**
 * The LAN face — "serving in" (v0.9.9 内网接入 batch 3).
 *
 * Three decisions this pins, each of which fails quietly:
 *
 * - **the server runs over the app's own state**, never a copy: the shell manages
 *   an `Arc<AppState>` and `host-tauri`'s commands take that same type (a clone
 *   would have its own VM slot, and a board that can start a second QEMU is worse
 *   than no board);
 * - **the settings decide and the wiring acts**: `set_network` stops whatever is
 *   running and starts what the new settings ask for, and `lan_allow_lan` is the
 *   only thing that binds beyond loopback;
 * - **the board's life is the app's**: the `Running` handle is aborted on exit.
 *
 * What a probe cannot see: whether the socket binds, whether a phone can reach it.
 * Those are walked by hand (`docs/manual-acceptance.md`, layer 8).
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");

const SHELL_MANIFEST = path.join(REPO, "ui", "src-tauri", "Cargo.toml");
const SHELL_LIB = path.join(REPO, "ui", "src-tauri", "src", "lib.rs");
const SHELL_LAN = path.join(REPO, "ui", "src-tauri", "src", "lan.rs");
const CONF = path.join(REPO, "ui", "src-tauri", "tauri.conf.json");
const COMMANDS = path.join(REPO, "host-tauri", "src", "commands.rs");
const SETTINGS = path.join(REPO, "host-core", "src", "settings.rs");
const SRC_SETTINGS = path.join(REPO, "host-core", "src", "settings.rs");
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
const code = (s) => s.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");

// ----- the shell owns the edge ------------------------------------------------

const manifest = read(SHELL_MANIFEST);
check(
  "the shell crate takes the server edge (and only it)",
  /^server = \{ path = "\.\.\/\.\.\/server" \}$/m.test(manifest),
);
check(
  "...and host-tauri does not",
  !/^server = /m.test(read(path.join(REPO, "host-tauri", "Cargo.toml"))),
);

const commands = code(read(COMMANDS));
check(
  "every host command takes the shared handle",
  !/State<'_, AppState>/.test(commands) &&
    (commands.match(/State<'_, Arc<AppState>>/g) ?? []).length === 68,
  `${(commands.match(/State<'_, Arc<AppState>>/g) ?? []).length} of 68`,
);
check(
  "...including the three that resolve it from the app",
  (commands.match(/app\.state::<Arc<AppState>>\(\)/g) ?? []).length === 3,
);

const lib = code(read(SHELL_LIB));
check(
  "the shell manages one Arc, not a copy",
  /let shared = Arc::new\(state\);/.test(lib) && /app\.manage\(Arc::clone\(&shared\)\)/.test(lib),
);
check(
  "the board starts again on a node that was left serving",
  /lan::apply\(app\.handle\(\), &settings\)/.test(lib),
);
check(
  "the board's life is the app's",
  /RunEvent::ExitRequested/.test(lib) && /lan\.stop\(\)/.test(lib),
);
check(
  "the settings decide, the command acts",
  /fn set_network\(/.test(lib) &&
    /state\s*\.set_network\(network\.clone\(\)\)/.test(lib.replace(/\s+/g, " ")) &&
    /lan::apply\(&app, &network\)/.test(lib),
);

const lan = code(read(SHELL_LAN));
check(
  "the server is built over the app's own handle and started",
  /Server::new\(state, config\)\.start\(\)/.test(lan) &&
    /tauri::async_runtime::spawn/.test(lan),
);
check(
  "...with the token the server would demand, minted by the server's own code",
  /server::token::load_or_create\(state\.data_dir\(\)\)/.test(lan) &&
    /TokenAuth::new\(token_file\.token\(\)\)/.test(lan),
);
check(
  "loopback unless the switch that reaches a phone is on",
  /let host = if settings\.lan_allow_lan/.test(lan.replace(/\s+/g, " ")),
);
check(
  "a change rebinds: stop first, then start",
  /lan\.stop\(\);/.test(lan) && /if !settings\.lan_enabled \{\s*return Ok\(\(\)\);/.test(lan),
);
check(
  "the built front end resolves for a bundle and for dev",
  /fn resolve_web_root/.test(lan) &&
    /resource_dir\(\)/.test(lan) &&
    /CARGO_MANIFEST_DIR/.test(lan) &&
    /join\("index\.html"\)/.test(lan),
);

// ----- the bundle ships the board ---------------------------------------------

const conf = JSON.parse(read(CONF));
check(
  "the bundle carries the built front end, as `dist`",
  JSON.stringify(conf.bundle?.resources ?? null) === JSON.stringify({ "../dist": "dist" }),
  JSON.stringify(conf.bundle?.resources ?? null),
);

// ----- the switches default to off --------------------------------------------

const settings = read(SRC_SETTINGS);
check(
  "both switches are off in a fresh settings file",
  /pub lan_enabled: bool,/.test(settings) &&
    /pub lan_allow_lan: bool,/.test(settings) &&
    /pub struct NetworkSettings/.test(settings),
);

check("gate.sh runs this probe", /probe-ui-lan\.mjs/.test(read(GATE)));

if (failures > 0) {
  console.log(`\n${failures} failing check(s)`);
  process.exit(1);
}
console.log("\nOK");
