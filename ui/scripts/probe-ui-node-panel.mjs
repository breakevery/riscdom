#!/usr/bin/env node
/**
 * The node page probe (v0.9 D2b-4b).
 *
 * Three things this batch claims, and each is checkable from the sources:
 *
 * - the node page is **tabs inside one view**, not three more views: `StatusPanel` holds
 *   a local `useState` over three sub-panels, and `AppShell`'s view union did not grow;
 * - the four node reads are **shared names** — the desktop has the commands, the browser
 *   has the endpoints — so `api/tauri.ts`, `api/http.ts` and `api/index.ts` all carry
 *   them (this probe names the command and the path, so a typo in either shows up);
 * - the browser's `SandboxView` really is the host's: every field of the Rust struct is
 *   present in the TypeScript interface (`sandbox_def.rs` is read, not taken on trust).
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const SRC = path.join(REPO, "ui", "src");
const RUST = path.join(REPO, "host-core", "src", "sandbox_def.rs");
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

const read = (relative) => readFileSync(path.join(SRC, relative), "utf8");
const readAbs = (absolute) => readFileSync(absolute, "utf8");
const count = (source, needle) => source.split(needle).length - 1;

// ----- three sub-panels, one view ---------------------------------------------

for (const panel of ["NodeStatus.tsx", "NodeExecutors.tsx", "NodeSandboxes.tsx"]) {
  const source = readFileSync(path.join(SRC, "panels", "node", panel), "utf8");
  check(
    `panels/node/${panel} exists and takes the store`,
    /export default function \w+\(\{ store \}: \{ store: AppStore \}\)/.test(source),
  );
}

const page = read("panels/StatusPanel.tsx");
check(
  "the node page has three tabs, held as local state",
  count(page, 'labelKey: "node.tab.') === 3 && /useState<NodeTab>\("status"\)/.test(page),
);
check(
  "…and renders whichever sub-panel is active",
  ["NodeStatus", "NodeExecutors", "NodeSandboxes"].every((name) => page.includes(`<${name} store={store} />`)),
);

const shell = read("layout/AppShell.tsx");
check(
  "AppShell's view union did not grow",
  /type View = "main" \| "settings" \| "status";/.test(shell),
);
check(
  "the tabs are the settings page's own row, so no new styling",
  page.includes('className="settings-tabs"') && page.includes('className={`tab-btn'),
);

// ----- four shared wrappers ---------------------------------------------------

const tauri = read("api/tauri.ts");
for (const [fn, command] of [
  ["listExecutors", "list_executors"],
  ["listSandboxes", "list_sandboxes"],
  ["currentSandbox", "current_sandbox"],
  ["sandboxCandidates", "sandbox_candidates"],
]) {
  check(
    `tauri.ts wraps the \`${command}\` command`,
    new RegExp(`export const ${fn} = \\(\\) => invoke<[^>]+>\\("${command}"\\)`).test(tauri),
  );
}

const http = read("api/http.ts");
for (const [fn, route] of [
  ["listExecutors", "/v0/executors"],
  ["listSandboxes", "/v0/sandboxes"],
  ["currentSandbox", "/v0/sandboxes/current"],
  ["sandboxCandidates", "/v0/sandboxes/candidates"],
]) {
  check(
    `http.ts asks for ${route}`,
    new RegExp(`export const ${fn} = \\(\\) => get<[^>]+>\\("${route.replace(/\//g, "\\/")}"\\)`).test(http),
  );
}

const index = read("api/index.ts");
check(
  "the adapter takes all four from whichever implementation is live",
  ["listExecutors", "listSandboxes", "currentSandbox", "sandboxCandidates"].every((fn) =>
    new RegExp(
      `export const ${fn}: SharedApi\\["${fn}"\\] = \\(\\.\\.\\.args\\) =>\\s*current\\.${fn}\\(\\.\\.\\.args\\);`,
    ).test(index),
  ),
);
check(
  "they are shared names, not Web-only ones",
  !/listExecutors|listSandboxes|currentSandbox|sandboxCandidates/.test(
    index.slice(index.indexOf("type SharedApi"), index.indexOf("let current")),
  ),
);

// ----- the shape really is the host's ----------------------------------------

const rust = readAbs(RUST);
const struct = rust.slice(rust.indexOf("pub struct SandboxView"), rust.indexOf("pub struct CandidateView"));
const rustFields = [...struct.matchAll(/^\s{4}pub (\w+):/gm)].map((m) => m[1]);
const types = read("api/types.ts");
const sandboxInterface = types.slice(
  types.indexOf("export interface SandboxView"),
  types.indexOf("export interface SandboxListResponse"),
);
const missing = rustFields.filter((field) => !new RegExp(`^\\s{2}${field}:`, "m").test(sandboxInterface));
check(
  "every Rust field of SandboxView is in the browser's shape",
  rustFields.length >= 10 && missing.length === 0,
  missing.length === 0 ? `${rustFields.length} fields` : `missing: ${missing.join(", ")}`,
);

// ----- the wording ------------------------------------------------------------

const registry = await import(pathToFileURL(path.join(SRC, "i18n", "strings.ts")).href);
const nodeKeys = Object.keys(registry.STRINGS.en).filter((key) => key.startsWith("node."));
const gaps = [];
for (const language of ["en", "zh"]) {
  for (const key of nodeKeys) {
    const value = registry.STRINGS[language]?.[key];
    if (typeof value !== "string" || value.trim() === "") gaps.push(`${language}:${key}`);
  }
}
check(
  "the node page's keys are complete in both languages",
  nodeKeys.length >= 18 && gaps.length === 0,
  gaps.length === 0 ? `${nodeKeys.length} keys` : gaps.join(", "),
);

check("gate.sh runs this probe", /probe-ui-node-panel\.mjs/.test(readAbs(GATE)));

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
