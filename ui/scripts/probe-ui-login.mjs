#!/usr/bin/env node
/**
 * The Web client's front door and its first page (v0.9 D2b-2).
 *
 * Two things can be checked without a browser, and they are the two that break
 * quietly:
 *
 * - **the gate**: `App` renders the login screen instead of the shell until a token
 *   is in hand, and it does so *outside* the store — a shell that mounts first would
 *   fire a dozen unauthenticated requests before anyone could type a token;
 * - **the words**: every key the login page and the status page ask the registry for
 *   exists in **both** languages. `check-ui-strings.mjs` proves the two tables have
 *   the same keys; nothing proves a *used* key is in either table.
 *
 * The status page's own wording is a pure rule (`lib/statusView.ts`), so it is
 * imported and exercised here as shipped, like the other `lib/` probes.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const SRC = path.join(REPO, "ui", "src");

const LOGIN = path.join(SRC, "Login.tsx");
const APP = path.join(SRC, "App.tsx");
const SHELL = path.join(SRC, "layout", "AppShell.tsx");
const STATUS_PANEL = path.join(SRC, "panels", "StatusPanel.tsx");
// The node page's status tab owns the wording since v0.9 D2b-4b (the page became a
// container with three sub-panels).
const NODE_STATUS = path.join(SRC, "panels", "node", "NodeStatus.tsx");
const STATUS_RULE = path.join(SRC, "lib", "statusView.ts");
const STRINGS = path.join(SRC, "i18n", "strings.ts");

let failures = 0;

function check(name, ok, detail) {
  if (ok) {
    console.log(`PASS  ${name}${detail ? `: ${detail}` : ""}`);
  } else {
    failures += 1;
    console.log(`FAIL  ${name}${detail ? `: ${detail}` : ""}`);
  }
}

/** The file's code, with comments removed: a name mentioned in a doc comment is not
 * a call, and several checks below are about what the code does. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");
}

const login = code(readFileSync(LOGIN, "utf8"));
const app = code(readFileSync(APP, "utf8"));
const shell = code(readFileSync(SHELL, "utf8"));
const panel = code(readFileSync(STATUS_PANEL, "utf8"));
const statusRuleSource = readFileSync(STATUS_RULE, "utf8");

// ----- the gate --------------------------------------------------------------

check(
  "the gate is in App, on the token the API already holds",
  /api\.currentToken\(\)/.test(app) && /<Login\b/.test(app) && /<AppShell\b/.test(app),
);

const gateIndex = app.indexOf("<Login");
const shellIndex = app.indexOf("<AppShell");
check(
  "the shell is rendered only on the other side of the gate",
  gateIndex > 0 && shellIndex > gateIndex,
  `Login@${gateIndex} AppShell@${shellIndex}`,
);
check(
  "the store is not mounted by the gate",
  !/useAppStore/.test(app),
  "useAppStore stays inside AppShell",
);
check(
  "the store is still called exactly once in the shell",
  (shell.match(/useAppStore\(\)/g) ?? []).length === 1,
);

// A candidate is proven **before** it is installed, or a refused token would be
// left in place for the next call.
const verifyIndex = login.indexOf("verifyToken(");
const installIndex = login.indexOf("setToken(");
check(
  "the login page verifies the candidate before installing it",
  verifyIndex > 0 && installIndex > verifyIndex,
  `verifyToken@${verifyIndex} setToken@${installIndex}`,
);
check(
  "the remember box is what the install is told",
  /setToken\(token,\s*remember\)/.test(login),
);
check(
  "the token never travels in a URL",
  !/location\.(href|search)|[?&]token=/.test(login),
);

// ----- the status page ---------------------------------------------------------

check(
  "the status panel takes the store, like every other panel",
  /export default function StatusPanel\(\{ store \}: \{ store: AppStore \}\)/.test(panel) &&
    !/useAppStore/.test(panel),
);
check(
  "the status panel decides nothing about wording itself",
  /from "\.\.\/\.\.\/lib\/statusView"/.test(readFileSync(NODE_STATUS, "utf8")),
  "the wording lives in panels/node/NodeStatus.tsx (v0.9 D2b-4b)",
);

const shellHasView =
  /type View = "main" \| "settings" \| "status"/.test(shell) &&
  /view === "status" \? "flex" : "none"/.test(shell) &&
  /<StatusPanel store=\{store\} \/>/.test(shell);
check("the status page is a third view of the same shell", shellHasView);
check(
  "the status entry is offered only where it can work",
  /const webClient = !isTauriRuntime\(\)/.test(shell) && /\{webClient \? \(/.test(shell),
);

// ----- the wording --------------------------------------------------------------

const registry = await import(pathToFileURL(STRINGS).href);
const statusRule = await import(pathToFileURL(STATUS_RULE).href);

/** Every `t("key")` a file asks for. */
function usedKeys(source) {
  return [...source.matchAll(/\bt\(\s*"([a-z0-9_.]+)"/g)].map((m) => m[1]);
}

const used = [
  ...new Set([...usedKeys(login), ...usedKeys(panel), ...usedKeys(statusRuleSource)]),
];
check(
  "the new screens ask for a bounded set of keys",
  used.length > 0 && used.length <= 40,
  `${used.length} distinct keys`,
);

const missing = [];
for (const language of ["en", "zh"]) {
  for (const key of used) {
    const value = registry.STRINGS[language]?.[key];
    if (typeof value !== "string" || value.trim() === "") {
      missing.push(`${language}:${key}`);
    }
  }
}
check(
  "every key they use is in both languages",
  missing.length === 0,
  missing.join(", ") || `${used.length} keys x 2 languages`,
);

check(
  "the login page offers three different failure sentences",
  ["login.unauthorized", "login.unreachable", "login.other"].every((key) =>
    login.includes(`"${key}"`),
  ),
);

check(
  "the gate's own code never calls the store's hook",
  !/useAppStore/.test(app),
);

// ----- the status rule, as shipped ---------------------------------------------

const node = statusRule.nodeFields({ status: "ok", version: "0.8.0", uptime_ms: 5000 });
check(
  "the node card prints the version it was handed",
  node.some((f) => f.value === "0.8.0"),
  node.map((f) => `${f.label}=${f.value}`).join(" "),
);
check(
  "an unread field prints a dash rather than vanishing",
  statusRule.nodeFields(null).every((f) => f.value === "\u2014") &&
    statusRule.counterFields(null).every((f) => f.value === "\u2014"),
);
const counters = statusRule.counterFields({
  status: "ok",
  version: "0.8.0",
  uptime_ms: 1,
  connections: 3,
  sse_subscribers: 0,
  agents: 1,
  agent_id: "local-4242-1",
});
check(
  "the counters card prints the node's numbers and its identity",
  counters.map((f) => f.value).join(",") === "3,0,1,local-4242-1",
  counters.map((f) => f.value).join(","),
);
check(
  "an uptime under a minute reads in seconds",
  statusRule.uptimeLabel(5_000).includes("5"),
  statusRule.uptimeLabel(5_000),
);
check(
  "an uptime past an hour reads in hours",
  statusRule.uptimeLabel(2 * 60 * 60 * 1000).includes("2"),
  statusRule.uptimeLabel(2 * 60 * 60 * 1000),
);

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
