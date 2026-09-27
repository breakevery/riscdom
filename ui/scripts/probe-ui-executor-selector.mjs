#!/usr/bin/env node
/**
 * The executor picker probe (v1.0 M2b-3b).
 *
 * Three things this batch claims, and each is checkable from the sources:
 *
 * - the picker is **two `<select>`s, not a new screen**: `ModelTab` gains one above its
 *   provider field and `ChatPanel` one in the session list's own row, both over the same
 *   `appStore` field, and the settings tab table did not move (`WEB_TABS` / `REMOTE_TABS`
 *   still exclude the model form from a browser and a remote window);
 * - the **empty value is this node** — the spelling both transports read as "no executor
 *   named" — so neither picker needs a second code path, and neither offers the wildcard
 *   (the "current session" is one executor's, so the endpoint refuses `*`);
 * - the **transports agree**: every wrapper whose endpoint or command takes an `executor`
 *   carries it in `api/tauri.ts` *and* `api/http.ts`, because `SharedApi` is derived from
 *   the HTTP module and a signature that drifted would not compile.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const SRC = path.join(REPO, "ui", "src");
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

// ----- two pickers, one store field ------------------------------------------

const model = read("settings/ModelTab.tsx");
check(
  "ModelTab renders an executor select from the store's own selection",
  /t\("model\.executor"\)/.test(model) &&
    /value=\{store\.executorSelection\}/.test(model) &&
    /store\.setExecutorSelection\(e\.target\.value\)/.test(model),
);
check(
  "…and it sits **above** the provider field, which is unchanged",
  model.indexOf('t("model.executor")') > 0 &&
    model.indexOf('t("model.executor")') < model.indexOf('t("model.provider")'),
);
check(
  "…with the local entry as the empty option, labelled by the registry",
  /<option value=""(?: |\n)/.test(model) && /t\("model\.executor_this_node"\)/.test(model),
);
check(
  "…and the fleet as the other options, keyed by the executor id",
  /store\.executors\.map\(\(executorOption\) => \(/.test(model) &&
    /value=\{executorOption\.agent_id\}/.test(model),
);
check(
  "…and no wildcard entry anywhere in the picker",
  !/value="\*"/.test(model),
);
check(
  "ModelTab asks for the fleet once, the same read the node page makes",
  /void refreshExecutors\(\);/.test(model),
);

const chat = read("panels/ChatPanel.tsx");
check(
  "ChatPanel renders an executor select inside the session list's row",
  /t\("chat\.executor"\)/.test(chat) &&
    /value=\{store\.executorSelection\}/.test(chat) &&
    /store\.setExecutorSelection\(e\.target\.value\)/.test(chat),
);
check(
  "…before the new-session button, in the list's own `.row`",
  chat.indexOf('<div className="row">') < chat.indexOf('t("chat.executor")') &&
    chat.indexOf('t("chat.executor")') < chat.indexOf('t("chat.session_new")'),
);
check(
  "…with the same local-first option list, and no wildcard",
  /<option value="">\{t\("chat\.executor_this_node"\)\}<\/option>/.test(chat) &&
    !/value="\*"/.test(chat),
);check(
  "…and ChatPanel asks for the fleet too",
  /void store\.refreshExecutors\(\);/.test(chat),
);

const store = read("state/appStore.ts");
check(
  "the store owns the selection, empty meaning this node",
  /const \[executorSelection, setExecutorSelection\] = useState\(""\);/.test(store),
);
// The field's own comment is where "empty is this node" is written down, so the
// assertion reads it rather than trusting the default alone.
check(
  "…and says so where the field is declared",
  /An \*\*empty string\*\* means this node's own/.test(store),
);
for (const [label, pattern] of [
  ["refreshLlmStatus", /api\.getLlmConfigStatus\(executorSelection \|\| undefined\)/],
  ["refreshSessions", /api\.listSessions\(50, executor\)/],
  ["refreshSessions (current)", /api\.getCurrentSessionId\(executor\)/],
  ["openSession", /api\.openSession\(id, executorSelection \|\| undefined\)/],
  ["newSession", /api\.createSession\(\s*t\("chat\.session_default_title"\),\s*executorSelection \|\| undefined,\s*\)/],
  ["renameSession", /api\.renameSession\(id, title, executorSelection \|\| undefined\)/],
  ["deleteSession", /api\.deleteSession\(id, executorSelection \|\| undefined\)/],
]) {
  check(`…and ${label} names it`, pattern.test(store));
}
check(
  "the selection is on the store's surface, with its setter",
  /^\s+executorSelection: string;$/m.test(store) &&
    /^\s+setExecutorSelection: \(next: string\) => void;$/m.test(store),
);

check(
  "the form follows the selection: a configured executor's values, else the defaults",
  /setBaseUrl\(status\.base_url\)/.test(model) &&
    /setBaseUrl\(DEFAULT_BASE_URL\)/.test(model) &&
    /setApiKey\(""\);/.test(model),
);

// ----- the model form edits the selected executor ----------------------------

for (const [label, pattern] of [
  ["readiness", /api\.getLlmReadiness\(executor\)/],
  ["status", /api\.getLlmConfigStatus\(executor\)/],
  ["the startup restore's stored-key probe", /api\.hasStoredKey\(pid, executor\)/],
  ["the startup restore's load", /api\.loadStoredKey\(pid, executor\)/],
  ["the banner loader", /await api\.loadStoredKey\(id, executor\);/],
  ["the provider-change probe", /api\.hasStoredKey\(id, executor\)/],
  ["the save", /api\.setLlmConfig\(apiKey, baseUrl, model, providerId, remember, executor\)/],
  ["the local-model switch", /api\.setLlmConfig\("", provider\.base_url, localModel, provider\.id, false, executor\)/],
  ["the clear", /await api\.clearLlmConfig\(executor\);/],
]) {
  check(`the model form's ${label} targets the selection`, pattern.test(model));
}

// ----- the transports agree --------------------------------------------------

const tauri = read("api/tauri.ts");
const http = read("api/http.ts");
const SHARED = [
  // (exported name, a fragment of the signature that must carry `executor`)
  ["getLlmConfigStatus", /getLlmConfigStatus = \(executor\?: string\)/],
  ["getLlmReadiness", /getLlmReadiness = \(executor\?: string\)/],
  ["setLlmConfig", /executor\?: string,/],
  ["hasStoredKey", /hasStoredKey = (?:async )?\(providerId: string, executor\?: string\)/],
  ["loadStoredKey", /loadStoredKey = \((?:_)?providerId: string, (?:_)?executor\?: string\)/],
  ["clearLlmConfig", /clearLlmConfig = \((?:_)?executor\?: string\)/],
  ["listSessions", /listSessions = \(limit: number, executor\?: string\)/],
  ["createSession", /createSession = \((?:_)?title: string, (?:_)?executor\?: string\)/],
  ["openSession", /(?:_)?sessionId: string,\s*(?:_)?executor\?: string\s*[,)]/],
  ["renameSession", /(?:_)?title: string,\s*(?:_)?executor\?: string\s*[,)]/],
  ["deleteSession", /(?:_)?sessionId: string,\s*(?:_)?executor\?: string\s*[,)]/],
  ["clearAllSessions", /clearAllSessions = \((?:_)?executor\?: string\)/],
  ["getCurrentSessionId", /getCurrentSessionId = (?:async )?\((?:executor\?: string)?\)/],
];
for (const [name, pattern] of SHARED) {
  check(
    `tauri.ts's ${name} takes an optional executor`,
    pattern.test(tauri),
  );
}
for (const [name, pattern] of SHARED) {
  check(
    `http.ts's ${name} takes an optional executor`,
    pattern.test(http),
  );
}

// The two node-wide LLM reads must **not** have grown the parameter: a provider preset
// and a loopback probe are the same whatever executor is asking.
check(
  "the two node-wide LLM reads are unchanged",
  /export const probeLocalLlm = \(\) =>/.test(tauri) &&
    /export const getProviderPresets = \(\) =>/.test(tauri) &&
    /export const probeLocalLlm = \(\) =>/.test(http) &&
    /export const getProviderPresets = \(\) =>/.test(http),
);

check(
  "the desktop's fleet read is adapted to the shared shape",
  /const labels = \(await invoke<string\[\]>\("list_executors"\)\) \?\? \[\];/.test(tauri) &&
    /return \{ executors: labels\.map\(\(agent_id\) => \(\{ agent_id \}\)\) \};/.test(tauri),
);

// `executor: ""` must reach the wire as *no parameter*, which is what makes the empty
// option mean this node. That property belongs to the query builder, so it is read here.
check(
  "an empty executor is left out of the query string",
  /if \(value === undefined \|\| value === ""\) continue;/.test(http),
);

// ----- the settings tab table did not move -----------------------------------

const tabs = read("settings/SettingsTabs.tsx");
check(
  "the browser still has no model form, and a remote window still has none",
  /const REMOTE_TABS: TabId\[\] = \["audit", "appearance", "network"\];/.test(tabs) &&
    /const WEB_TABS: TabId\[\] = \["audit", "appearance"\];/.test(tabs),
);
check(
  "…so the picker cannot appear where the form does not",
  tabs.includes("isLocalHost()") && tabs.includes("<DesktopOnly>"),
);

// ----- the wording -----------------------------------------------------------

const registry = await import(
  pathToFileURL(path.join(SRC, "i18n", "strings.ts")).href
);
const KEYS = [
  "model.executor",
  "model.executor_this_node",
  "model.executor_hint",
  "chat.executor",
  "chat.executor_this_node",
  "chat.executor_hint",
];
const gaps = [];
for (const language of ["en", "zh"]) {
  for (const key of KEYS) {
    const value = registry.STRINGS[language]?.[key];
    if (typeof value !== "string" || value.trim() === "") gaps.push(`${language}:${key}`);
  }
}
check(
  "the picker's keys are complete in both languages",
  gaps.length === 0,
  gaps.length === 0 ? `${KEYS.length} keys` : gaps.join(", "),
);
check(
  "the node's own entry reads as this node in both languages",
  registry.STRINGS.en["model.executor_this_node"] === "This node" &&
    registry.STRINGS.zh["model.executor_this_node"] === "本机",
);

check("gate.sh runs this probe", /probe-ui-executor-selector\.mjs/.test(readAbs(GATE)));

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
