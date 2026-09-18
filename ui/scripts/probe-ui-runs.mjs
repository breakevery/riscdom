#!/usr/bin/env node
/**
 * Run-list probe (v0.4 batch 1d).
 *
 * Checks what the settings → Audit tab renders for the host's runs, and that the
 * two `invoke` names the UI calls are actually registered in the Tauri shell
 * (a typo there fails silently at runtime, which is exactly the kind of thing a
 * probe should catch).
 *
 * The rendering rules live in `ui/src/lib/runView.ts` — a dependency-free module —
 * so this imports the **shipped** code directly (Node strips the types).
 *
 *   node ui/scripts/probe-ui-runs.mjs
 */
import { readFileSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const MODULE = path.join(REPO, "ui", "src", "lib", "runView.ts");
const TAB = path.join(REPO, "ui", "src", "settings", "AuditTab.tsx");
const API = path.join(REPO, "ui", "src", "api", "tauri.ts");
const SHELL = path.join(REPO, "ui", "src-tauri", "src", "lib.rs");

let failures = 0;

function check(name, ok, detail) {
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `: ${detail}` : ""}`);
}

const runs = await import(pathToFileURL(MODULE).href);
console.log(`# run list (${path.relative(REPO, MODULE)})\n`);

const now = 1_700_000_000_000;
const mock = [
  {
    run_id: "run_old",
    status: "ok",
    fingerprint_short: "3173d3e8532f632d",
    parent_run_id: null,
    session_id: "s1",
    started_at_ms: now - 3 * 60 * 60 * 1000,
    ended_at_ms: now - 3 * 60 * 60 * 1000 + 4_000,
  },
  {
    run_id: "run_new",
    status: "failed",
    fingerprint_short: "aaaaaaaaaaaaaaaa",
    parent_run_id: "run_old",
    session_id: "s1",
    started_at_ms: now - 30 * 1000,
    ended_at_ms: now - 29 * 1000,
  },
  {
    run_id: "run_open",
    status: "open",
    fingerprint_short: "bbbbbbbbbbbbbbbb",
    parent_run_id: null,
    session_id: null,
    started_at_ms: now - 5 * 1000,
    ended_at_ms: null,
  },
];

const rows = runs.toRunRows(mock, now);
check("one row per run", rows.length === 3, `${rows.length}`);
check(
  "newest first",
  rows.map((r) => r.runId).join(",") === "run_open,run_new,run_old",
  rows.map((r) => r.runId).join(","),
);
check("status labels are human", rows[0].statusLabel === "进行中" && rows[1].statusLabel === "失败");
check("the fingerprint is shown", rows[2].fingerprint === "3173d3e8532f632d");
check("a restored run shows its parent", rows[1].parentLabel === "← run_old", rows[1].parentLabel);
check("a plain run shows no parent", rows[0].parentLabel === "" && rows[2].parentLabel === "");
check("recent runs read as recent", rows[0].whenLabel === "5 秒前", rows[0].whenLabel);
check("older runs read as hours", rows[2].whenLabel === "3 小时前", rows[2].whenLabel);
check("an unknown status is passed through", runs.statusLabel("weird") === "weird");
check("the empty state says something", runs.runsEmptyText().length > 0);

// Wiring: the tab uses the shipped rule, and the calls resolve to real commands.
const tab = readFileSync(TAB, "utf8");
check("AuditTab uses the run helpers", /from\s+"\.\.\/lib\/runView"/.test(tab));
check("AuditTab renders the rows", /toRunRows\(store\.runs/.test(tab));
check("AuditTab has an empty state", /runsEmptyText\(\)/.test(tab));

const api = readFileSync(API, "utf8");
const shell = readFileSync(SHELL, "utf8");
for (const [name, command] of [
  ["listRuns", "list_runs"],
  ["getRun", "get_run"],
]) {
  check(`the ${name} wrapper calls \`${command}\``, api.includes(`"${command}"`));
  check(`\`${command}\` is registered in the shell`, shell.includes(`commands::${command}`));
}

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
