#!/usr/bin/env node
/**
 * The API adapter probe (v0.9 D2b-1).
 *
 * `ui/src/api/` has two implementations of one surface: the desktop's Tauri
 * commands and the browser's HTTP endpoints. This probe holds them to each other
 * and holds the HTTP one to the wire:
 *
 * - the two implementations export the **same names** (checked on the source text,
 *   so neither has to be imported through Tauri here);
 * - the 26 read-only calls go to the documented path with the documented query, with
 *   `fetch` replaced by a stand-in — no network, loopback or otherwise;
 * - the eight one-field wrappers (`{"theme": …}` and friends) are unwrapped;
 * - the 26 controls reject with a sentence instead of pretending to work, and the
 *   subscriptions return a no-op unsubscribe instead of rejecting;
 * - a refusal keeps the host's error model in one line, and the token rides on the
 *   request once one is set;
 * - the three consumers import the adapter, not one implementation of it.
 *
 * `http.ts` is dependency-free (types only), so Node can import the **shipped**
 * code directly (Node >= 22.6 strips the types).
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const API = path.join(REPO, "ui", "src", "api");
const HTTP = path.join(API, "http.ts");
const TAURI = path.join(API, "tauri.ts");
const INDEX = path.join(API, "index.ts");

let failures = 0;

function check(name, ok, detail) {
  if (ok) {
    console.log(`PASS  ${name}${detail ? `: ${detail}` : ""}`);
  } else {
    failures += 1;
    console.log(`FAIL  ${name}${detail ? `: ${detail}` : ""}`);
  }
}

/** The names a module exports as `export const x` or `export function x`. */
function exportedNames(source) {
  return [...source.matchAll(/^export (?:const|(?:async )?function) (\w+)/gm)]
    .map((m) => m[1])
    .sort();
}

/**
 * The names only the Web implementation carries.
 *
 * Every one of them is a **client** concern — where a token is kept, whether a
 * typed token is accepted, what the node says about itself, and where the Web
 * client's own event stream reports lost frames — and the desktop has no use for
 * any: it is in the same process as its host, always authenticated by being so.
 * They are listed here, and in `api/index.ts`'s `SharedApi`, so the two lists have
 * to agree.
 */
const WEB_ONLY = [
  "clearToken",
  "currentToken",
  "getHealth",
  "getStatus",
  "onGap",
  "setApiBase",
  "setToken",
  "verifyToken",
].sort();

// ----- the two implementations carry the same names --------------------------

const httpSource = readFileSync(HTTP, "utf8");
const tauriSource = readFileSync(TAURI, "utf8");
const indexSource = readFileSync(INDEX, "utf8");

const httpNames = exportedNames(httpSource);
const tauriNames = exportedNames(tauriSource);
const onlyHttp = httpNames.filter((name) => !tauriNames.includes(name));
const onlyTauri = tauriNames.filter((name) => !httpNames.includes(name));
check(
  "every name the desktop carries is carried by the Web client too",
  onlyTauri.length === 0,
  onlyTauri.join(",") || `${tauriNames.length} shared names`,
);
check(
  "the Web client's own names are exactly the declared list",
  JSON.stringify(onlyHttp) === JSON.stringify(WEB_ONLY),
  `only-http: ${onlyHttp.join(",") || "none"}; declared: ${WEB_ONLY.join(",")}`,
);

const READS = [
  "getAuditStatus",
  "listAuditEvents",
  "listRuns",
  "getRun",
  "compareRunFingerprints",
  "getProviderPresets",
  "getLlmConfigStatus",
  "getLlmReadiness",
  "probeLocalLlm",
  "hasStoredKey",
  "listSessions",
  "getCurrentSessionId",
  "listSnapshots",
  "vmIsRunning",
  "vmStatus",
  "probeToolchain",
  "toolchainDownloadStatus",
  "probeQemu",
  "getQemuStatus",
  "preflightStatus",
  "getTheme",
  "getLanguage",
  "getWorkspaceRoot",
  "getWorkspaceFiles",
  "readWorkspaceFile",
  "getSerialBuffer",
  "listExecutors",
  "listSandboxes",
  "currentSandbox",
  "sandboxCandidates",
];
const CONTROLS = [
  "setTheme",
  "setLanguage",
  "setAuditAlert",
  "runPreflight",
  "acknowledgePreflight",
  "setLlmConfig",
  "loadStoredKey",
  "clearLlmConfig",
  "runAgent",
  "exportSerialLog",
  "exportAuditJsonl",
  "exportRunAudit",
  "deleteSnapshot",
  "saveSnapshotReal",
  "resumeFromSnapshotReal",
  "startToolchainDownload",
  "cancelToolchainDownload",
  "setQemuPath",
  "clearQemuPath",
  "setToolchainPath",
  "clearToolchainPath",
  "createSession",
  "openSession",
  "renameSession",
  "deleteSession",
  "clearAllSessions",
  // The network face (v0.9.9 内网接入) is the desktop's: the browser can look at a
  // node, it cannot rewire one. They reject with a sentence like the controls.
  "getNetwork",
  "setNetwork",
  "readLanToken",
];
const EVENTS = [
  "onHostEvent",
  "onToolchainDownload",
  "onAgentStreamDelta",
  "onAgentStreamDone",
];

check(
  "the read-only surface is the 30 the UI needs",
  READS.every((name) => httpNames.includes(name)) && READS.length === 30,
  `${READS.length} names`,
);
check(
  "the controls and the subscriptions are accounted for",
  [...CONTROLS, ...EVENTS].every((name) => tauriNames.includes(name)) &&
    CONTROLS.length + EVENTS.length + READS.length === tauriNames.length,
  `${CONTROLS.length} controls + ${EVENTS.length} subscriptions + ${READS.length} reads = ${tauriNames.length}`,
);

// ----- `fetch` replaced by a stand-in ----------------------------------------

const seen = [];
let reply = { status: 200, body: {} };

globalThis.fetch = async (url, init) => {
  seen.push({ url, init });
  const body = JSON.stringify(reply.body);
  return {
    ok: reply.status >= 200 && reply.status < 300,
    status: reply.status,
    statusText: reply.status === 200 ? "OK" : "Error",
    json: async () => JSON.parse(body),
  };
};

/** The one request a call made (and the path it went to). */
async function one(call, body = {}) {
  seen.length = 0;
  reply = { status: 200, body };
  const value = await call();
  const url = seen.length === 1 ? seen[0].url : `(${seen.length} requests)`;
  return { value, url, init: seen[0]?.init };
}

const http = await import(pathToFileURL(HTTP).href);

// ----- each read goes to its documented path ---------------------------------

const READ_CALLS = [
  ["getAuditStatus", () => http.getAuditStatus(), "/v0/audit/status"],
  ["listRuns", () => http.listRuns(20), "/v0/runs?limit=20"],
  ["getRun", () => http.getRun("run_1"), "/v0/runs/run_1"],
  [
    "compareRunFingerprints",
    () => http.compareRunFingerprints("a", "b"),
    "/v0/runs/diff?run_a=a&run_b=b",
  ],
  [
    "listAuditEvents",
    () => http.listAuditEvents(50, "host", "vm."),
    "/v0/audit/events?limit=50&actor=host&action_prefix=vm.",
  ],
  ["getProviderPresets", () => http.getProviderPresets(), "/v0/llm/provider-presets"],
  ["getLlmConfigStatus", () => http.getLlmConfigStatus(), "/v0/llm/config"],
  ["getLlmReadiness", () => http.getLlmReadiness(), "/v0/llm/readiness"],
  ["probeLocalLlm", () => http.probeLocalLlm(), "/v0/llm/local-probe"],
  [
    "hasStoredKey",
    () => http.hasStoredKey("deepseek"),
    "/v0/llm/stored-key?provider_id=deepseek",
  ],
  ["listSessions", () => http.listSessions(50), "/v0/sessions?limit=50"],
  ["getCurrentSessionId", () => http.getCurrentSessionId(), "/v0/sessions/current"],
  ["listSnapshots", () => http.listSnapshots(), "/v0/snapshots"],
  ["vmIsRunning", () => http.vmIsRunning(), "/v0/vm/running"],
  ["vmStatus", () => http.vmStatus(), "/v0/vm/status"],
  ["probeToolchain", () => http.probeToolchain(), "/v0/toolchain"],
  [
    "toolchainDownloadStatus",
    () => http.toolchainDownloadStatus(),
    "/v0/toolchain/download",
  ],
  ["probeQemu", () => http.probeQemu(), "/v0/qemu"],
  ["getQemuStatus", () => http.getQemuStatus(), "/v0/qemu/status"],
  ["preflightStatus", () => http.preflightStatus(), "/v0/preflight"],
  ["getTheme", () => http.getTheme(), "/v0/settings/theme"],
  ["getLanguage", () => http.getLanguage(), "/v0/settings/language"],
  ["getWorkspaceRoot", () => http.getWorkspaceRoot(), "/v0/workspace/root"],
  ["getWorkspaceFiles", () => http.getWorkspaceFiles(), "/v0/workspace/files"],
  [
    "readWorkspaceFile",
    () => http.readWorkspaceFile("src/main.c"),
    "/v0/workspace/file?path=src%2Fmain.c",
  ],
  ["getSerialBuffer", () => http.getSerialBuffer(), "/v0/serial"],
  ["listExecutors", () => http.listExecutors(), "/v0/executors"],
  ["listSandboxes", () => http.listSandboxes(), "/v0/sandboxes"],
  ["currentSandbox", () => http.currentSandbox(), "/v0/sandboxes/current"],
  [
    "sandboxCandidates",
    () => http.sandboxCandidates(),
    "/v0/sandboxes/candidates",
  ],
];

check(
  "every read-only call is covered by this table",
  READ_CALLS.length === READS.length && READ_CALLS.every(([name]) => READS.includes(name)),
  `${READ_CALLS.length} of ${READS.length}`,
);

for (const [name, call, wantPath] of READ_CALLS) {
  const { url, init } = await one(call);
  check(`${name} asks for ${wantPath}`, url === wantPath, url);
  check(
    `${name} sends no body and no content type`,
    init.method === "GET" && init.body === undefined,
    `${init.method}${init.body === undefined ? "" : " with a body"}`,
  );
}

// ----- the eight one-field wrappers are unwrapped ----------------------------

const UNWRAPS = [
  ["getTheme", () => http.getTheme(), { theme: "dark" }, "dark"],
  ["getLanguage", () => http.getLanguage(), { language: "zh" }, "zh"],
  ["getWorkspaceRoot", () => http.getWorkspaceRoot(), { root: "/ws" }, "/ws"],
  [
    "readWorkspaceFile",
    () => http.readWorkspaceFile("a.c"),
    { content: "int main(void) {}" },
    "int main(void) {}",
  ],
  ["getSerialBuffer", () => http.getSerialBuffer(), { buffer: "boot\n" }, "boot\n"],
  ["vmIsRunning", () => http.vmIsRunning(), { running: true }, true],
  ["hasStoredKey", () => http.hasStoredKey("x"), { present: false }, false],
  [
    "getCurrentSessionId",
    () => http.getCurrentSessionId(),
    { session_id: "sess-1" },
    "sess-1",
  ],
];

check("the unwrap table covers the eight wrappers", UNWRAPS.length === 8, `${UNWRAPS.length}`);

for (const [name, call, body, want] of UNWRAPS) {
  const { value } = await one(call, body);
  check(`${name} hands back the value, not the wrapper`, value === want, JSON.stringify(value));
}

// ----- a refusal is one line, and the token rides along ----------------------

seen.length = 0;
reply = {
  status: 404,
  body: { code: "not_found", message: "no run run_x in this log", retryable: false, cause: null },
};
const refusal = await http
  .getRun("run_x")
  .then(() => null)
  .catch((e) => e);
check(
  "a refusal is the host's sentence, as one string",
  typeof refusal === "string" && refusal === "not_found: no run run_x in this log",
  typeof refusal === "string" ? refusal : `a ${typeof refusal}`,
);

http.setToken("a-token");
const { init } = await one(() => http.getTheme(), { theme: "dark" });
check(
  "the token is presented once one is set",
  init.headers.Authorization === "Bearer a-token",
  String(init.headers.Authorization),
);
http.setToken("");

// ----- controls reject, subscriptions do not --------------------------------

const rejected = [];
// Two of the controls are implemented over HTTP on purpose (v0.9 D2b-4a, decision §62):
// theme and language are display preferences, not node configuration. The rest are
// desktop-only and must say so instead of pretending.
const WEB_IMPLEMENTED = ["setTheme", "setLanguage"];
for (const name of CONTROLS.filter((entry) => !WEB_IMPLEMENTED.includes(entry))) {
  const outcome = await http[name](..."xxxxx")
    .then(() => "resolved")
    .catch((e) => (typeof e === "string" ? e : `a ${typeof e}`));
  if (outcome === "resolved") {
    rejected.push(`${name} resolved`);
  } else if (typeof outcome !== "string" || !outcome.includes("desktop control")) {
    rejected.push(`${name} rejected with: ${outcome}`);
  }
}
check(
  "every control rejects with a sentence (and never resolves)",
  rejected.length === 0,
  rejected.join("; ") || `${CONTROLS.length - WEB_IMPLEMENTED.length} controls`,
);

// ----- the two controls the browser does implement ----------------------------

for (const [name, path_, key] of [
  ["setTheme", "/v0/settings/theme", "theme"],
  ["setLanguage", "/v0/settings/language", "language"],
]) {
  seen.length = 0;
  reply = { status: 204, body: {} };
  const settled = await http[name]("value")
    .then(() => "resolved")
    .catch((e) => `rejected: ${String(e)}`);
  const sent = seen[0];
  const body = sent === undefined ? null : JSON.parse(sent.init.body);
  check(
    `${name} writes a display preference over HTTP`,
    settled === "resolved" &&
      sent?.init.method === "POST" &&
      sent?.url === path_ &&
      body?.[key] === "value",
    `${settled}; ${sent?.init.method} ${sent?.url} ${sent?.init.body}`,
  );
}

const subscriptionProblems = [];
for (const name of EVENTS) {
  try {
    const off = http[name]("event", () => {});
    if (typeof off !== "function") subscriptionProblems.push(`${name} -> ${typeof off}`);
    else off();
  } catch (e) {
    subscriptionProblems.push(`${name} threw: ${String(e)}`);
  }
}
check(
  "every subscription returns an unsubscribe instead of throwing",
  subscriptionProblems.length === 0,
  subscriptionProblems.join("; ") || `${EVENTS.length} subscriptions`,
);

// ----- the consumers import the adapter --------------------------------------

for (const relative of [
  "src/state/appStore.ts",
  "src/panels/ChatPanel.tsx",
  "src/panels/CanvasPanel.tsx",
  "src/settings/ModelTab.tsx",
]) {
  const source = readFileSync(path.join(REPO, "ui", relative), "utf8");
  check(
    `${relative} imports the adapter, not one implementation`,
    /import \* as api from "\.\.\/api"/.test(source) && !source.includes("api/tauri"),
    "",
  );
}

check(
  "the adapter picks by Tauri's own global",
  /__TAURI_INTERNALS__/.test(indexSource) && /isTauriRuntime/.test(indexSource),
  "",
);
check(
  "the adapter takes every shared name from whichever implementation is live",
  JSON.stringify(exportedNames(indexSource)) ===
    JSON.stringify([...READS, ...CONTROLS, ...EVENTS, "isTauriRuntime"].sort()),
  `${exportedNames(indexSource).length} names`,
);

// The one rule that is not a transport call, so it has one implementation instead
// of two: the envelope unwrap, shared through `api/envelope.ts`.
const SHARED = 'export { unwrapHostPayload } from "./envelope.ts"';
check(
  "the shared envelope rule is re-exported by all three modules",
  httpSource.includes(SHARED) && tauriSource.includes(SHARED) && indexSource.includes(SHARED),
  `${httpNames.length} transport names + 1 shared rule`,
);

// ----- the token: where it lives, and how a candidate is checked ----------------

/** A stand-in `Storage`, the way a browser would provide one. */
function fakeStorage() {
  const values = new Map();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => void values.set(key, String(value)),
    removeItem: (key) => void values.delete(key),
    has: (key) => values.has(key),
  };
}

const session = fakeStorage();
const local = fakeStorage();
globalThis.sessionStorage = session;
globalThis.localStorage = local;
const KEY = "riscdom.web.token";

http.setToken("a-session-token", false);
check(
  "a token is kept for the session by default",
  session.has(KEY) && !local.has(KEY),
  `session=${session.has(KEY)} local=${local.has(KEY)}`,
);
http.setToken("a-remembered-token", true);
check(
  "a remembered token moves to the persistent store and leaves the other",
  local.has(KEY) && !session.has(KEY),
  `session=${session.has(KEY)} local=${local.has(KEY)}`,
);
check("the token in force is the remembered one", http.currentToken() === "a-remembered-token");
http.clearToken();
check(
  "log-out empties both stores",
  !session.has(KEY) && !local.has(KEY) && http.currentToken() === "",
  `session=${session.has(KEY)} local=${local.has(KEY)}`,
);

/** One `/v0/health` answer, as `verifyToken` sees it. */
async function checkToken(status, body, throws = false) {
  seen.length = 0;
  reply = { status, body, throw: throws };
  return http.verifyToken("candidate");
}

globalThis.fetch = async (url, init) => {
  seen.push({ url, init });
  if (reply.throw) throw new Error("connect ECONNREFUSED");
  return {
    ok: reply.status >= 200 && reply.status < 300,
    status: reply.status,
    statusText: "OK",
    json: async () => JSON.parse(JSON.stringify(reply.body)),
  };
};

const accepted = await checkToken(200, { status: "ok", version: "0.9.0", uptime_ms: 1234 });
check(
  "a good token comes back as ok, with the version",
  accepted.kind === "ok" && accepted.version === "0.9.0",
  JSON.stringify(accepted),
);
check(
  "the candidate rides the check and is not installed by it",
  seen[0].url === "/v0/health" &&
    seen[0].init.headers.Authorization === "Bearer candidate" &&
    http.currentToken() === "",
  `${seen[0].init.headers.Authorization}; current=${http.currentToken() || "(empty)"}`,
);
check(
  "a refused token is its own answer",
  (await checkToken(401, { code: "unauthorized", message: "nope" })).kind === "unauthorized",
);
check(
  "another status is its own answer",
  (await checkToken(400, { code: "bad_request", message: "nope" })).kind === "other",
);
const unreachable = await checkToken(200, {}, true);
check(
  "a server that does not answer is its own answer too",
  unreachable.kind === "unreachable",
  JSON.stringify(unreachable),
);

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
