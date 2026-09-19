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
const STORE = path.join(REPO, "ui", "src", "state", "appStore.ts");

let failures = 0;

function check(name, ok, detail) {
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `: ${detail}` : ""}`);
}

const runs = await import(pathToFileURL(MODULE).href);
console.log(`# run list (${path.relative(REPO, MODULE)})\n`);

const now = 1_700_000_000_000;
const full = (short) => short + "0".repeat(64 - short.length);
const mock = [
  {
    run_id: "run_old",
    status: "ok",
    fingerprint: full("3173d3e8532f632d"),
    fingerprint_short: "3173d3e8532f632d",
    parent_run_id: null,
    session_id: "s1",
    resumed_from_snapshot: null,
    started_at_ms: now - 3 * 60 * 60 * 1000,
    ended_at_ms: now - 3 * 60 * 60 * 1000 + 4_000,
  },
  {
    run_id: "run_new",
    status: "failed",
    fingerprint: full("aaaaaaaaaaaaaaaa"),
    fingerprint_short: "aaaaaaaaaaaaaaaa",
    parent_run_id: "run_old",
    session_id: "s1",
    resumed_from_snapshot: "s20c",
    started_at_ms: now - 30 * 1000,
    ended_at_ms: now - 29 * 1000,
  },
  {
    run_id: "run_open",
    status: "open",
    fingerprint: full("bbbbbbbbbbbbbbbb"),
    fingerprint_short: "bbbbbbbbbbbbbbbb",
    parent_run_id: null,
    session_id: null,
    resumed_from_snapshot: null,
    started_at_ms: now - 5 * 1000,
    ended_at_ms: null,
  },
  {
    // v0.5 batch 2: no `run.end`, but the chain has a `host.run.abandoned` marker.
    run_id: "run_abandoned",
    status: "abandoned",
    fingerprint: full("cccccccccccccccc"),
    fingerprint_short: "cccccccccccccccc",
    parent_run_id: null,
    session_id: null,
    resumed_from_snapshot: null,
    started_at_ms: now - 4 * 60 * 60 * 1000,
    ended_at_ms: null,
  },
];

const rows = runs.toRunRows(mock, now);
check("one row per run", rows.length === 4, `${rows.length}`);
check(
  "newest first",
  rows.map((r) => r.runId).join(",") === "run_open,run_new,run_old,run_abandoned",
  rows.map((r) => r.runId).join(","),
);
check("status labels are human", rows[0].statusLabel === "进行中" && rows[1].statusLabel === "失败");
check("the fingerprint is shown", rows[2].fingerprint === "3173d3e8532f632d");
check("a restored run shows its parent", rows[1].parentLabel === "← run_old", rows[1].parentLabel);
check("a plain run shows no parent", rows[0].parentLabel === "" && rows[2].parentLabel === "");
// v0.5 batch 3: the run row names the snapshot it was restored from.
check(
  "a restored run names its source snapshot",
  rows[1].snapshotLabel === "恢复自 s20c",
  rows[1].snapshotLabel,
);
check(
  "a run from scratch names no snapshot",
  rows[0].snapshotLabel === "" && rows[2].snapshotLabel === "" && rows[3].snapshotLabel === "",
);
check("the snapshot label helper is the shipped one", runs.snapshotLabel(null) === "" && runs.snapshotLabel("s1c") === "恢复自 s1c");
check("recent runs read as recent", rows[0].whenLabel === "5 秒前", rows[0].whenLabel);
check("older runs read as hours", rows[2].whenLabel === "3 小时前", rows[2].whenLabel);
check("an unknown status is passed through", runs.statusLabel("weird") === "weird");
check("the empty state says something", runs.runsEmptyText().length > 0);

// Export (v0.5 batches 1-2): any run the chain has closed can be exported.
check(
  "a finished run is exportable",
  rows.every((r) => (r.runId === "run_open" ? !r.exportable : r.exportable)),
  JSON.stringify(rows.map((r) => [r.runId, r.exportable])),
);
check(
  "an open run offers no export",
  rows.find((r) => r.runId === "run_open").exportable === false,
);
check(
  "an abandoned run offers an export",
  rows.find((r) => r.runId === "run_abandoned").exportable === true,
);
check("an abandoned run is labelled", rows[3].statusLabel === "已放弃", rows[3].statusLabel);

// The export's default path comes from the host's workspace root (v0.5 batch 2).
check("the default export path is root-scoped", runs.defaultExportPath("/ws", "run_a") === "/ws/run_a.audit.jsonl", runs.defaultExportPath("/ws", "run_a"));
check(
  "a windows root keeps its own separator",
  runs.defaultExportPath("C:\\ws", "run_a") === "C:\\ws\\run_a.audit.jsonl",
  runs.defaultExportPath("C:\\ws", "run_a"),
);
check(
  "a trailing separator is not doubled",
  runs.defaultExportPath("/ws/", "run_a") === "/ws/run_a.audit.jsonl",
  runs.defaultExportPath("/ws/", "run_a"),
);
check(
  "without a root the bare name is used",
  runs.defaultExportPath(null, "run_a") === "run_a.audit.jsonl",
  runs.defaultExportPath(null, "run_a"),
);

// Side-by-side comparison (v0.5 batch 2, step 7).
check("a click selects a run", runs.toggleRunSelection([], "a").join(",") === "a");
check("a second click selects a pair", runs.toggleRunSelection(["a"], "b").join(",") === "a,b");
check(
  "a third click replaces the oldest",
  runs.toggleRunSelection(["a", "b"], "c").join(",") === "b,c",
  runs.toggleRunSelection(["a", "b"], "c").join(","),
);
check("clicking a selected run removes it", runs.toggleRunSelection(["a", "b"], "a").join(",") === "b");
check(
  "the panel needs exactly two runs",
  runs.comparePanelVisible([]) === false &&
    runs.comparePanelVisible(["a"]) === false &&
    runs.comparePanelVisible(["a", "b"]) === true,
);
const compare = runs.toCompareRows(mock, ["run_new", "run_old"]);
check(
  "compare columns follow the click order",
  compare.map((c) => c.runId).join(",") === "run_new,run_old",
  compare.map((c) => c.runId).join(","),
);
check(
  "a compare column shows the short and the full digest",
  compare[0].fingerprintShort === "aaaaaaaaaaaaaaaa" && compare[0].fingerprint.length === 64,
  `${compare[0].fingerprintShort}/${compare[0].fingerprint.length}`,
);
check(
  "a compare column stamps the start time",
  /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(compare[0].startedLabel),
  compare[0].startedLabel,
);
check(
  "compare drops an id that is no longer listed",
  runs.toCompareRows(mock, ["run_new", "run_gone"]).length === 1,
);
check(
  "a compare column names the source snapshot too",
  compare[0].snapshotLabel === "恢复自 s20c" && compare[1].snapshotLabel === "",
  `${compare[0].snapshotLabel}/${compare[1].snapshotLabel}`,
);

// Wiring: the tab uses the shipped rule, and the calls resolve to real commands.
const tab = readFileSync(TAB, "utf8");
const store = readFileSync(STORE, "utf8");
check("AuditTab uses the run helpers", /from\s+"\.\.\/lib\/runView"/.test(tab));
check("AuditTab renders the rows", /toRunRows\(store\.runs/.test(tab));
check("AuditTab has an empty state", /runsEmptyText\(\)/.test(tab));

const api = readFileSync(API, "utf8");
const shell = readFileSync(SHELL, "utf8");
for (const [name, command] of [
  ["listRuns", "list_runs"],
  ["getRun", "get_run"],
  ["exportRunAudit", "export_run_audit"],
  ["getWorkspaceRoot", "workspace_root"],
]) {
  check(`the ${name} wrapper calls \`${command}\``, api.includes(`"${command}"`));
  check(`\`${command}\` is registered in the shell`, shell.includes(`commands::${command}`));
}

// The export button is wired to the wrapper with that row's run id.
check(
  "AuditTab offers an export per exportable run",
  /r\.exportable\s*\?/.test(tab) && /store\.exportRunAudit\(r\.runId\)/.test(tab),
);
check(
  "the export call passes the run id and the chosen path",
  /exportRunAudit\(runId, target\)/.test(store),
  "store must call api.exportRunAudit(runId, target)",
);

// The default path is built from the host's workspace root, never hard-coded, and
// the selection rule is the shipped one (v0.5 batch 2).
check(
  "the export default comes from the host's workspace root",
  /defaultPath:\s*defaultExportPath\(workspaceRoot, runId\)/.test(store),
  "appStore must call defaultExportPath(workspaceRoot, runId)",
);
check(
  "the store imports the export-path and selection rules",
  /from\s+"\.\.\/lib\/runView"/.test(store) &&
    /defaultExportPath/.test(store) &&
    /nextRunSelection\(prev, runId\)/.test(store),
);
check(
  "the store keeps the selection state",
  /const \[selectedRuns, setSelectedRuns\]/.test(store) && /selectedRuns,/.test(store),
);

check(
  "AuditTab renders the side-by-side panel",
  /comparePanelVisible\(store\.selectedRuns\)/.test(tab) &&
    /toCompareRows\(store\.runs, store\.selectedRuns\)/.test(tab),
);
check(
  "AuditTab offers a selection per run",
  /type="checkbox"/.test(tab) && /store\.toggleRunSelection\(r\.runId\)/.test(tab),
);
check(
  "AuditTab renders the run's source snapshot",
  /r\.snapshotLabel/.test(tab) && /c\.snapshotLabel/.test(tab),
);
check(
  "the RunView shape carries the source snapshot",
  /resumed_from_snapshot:\s*string\s*\|\s*null/.test(api),
);

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
