#!/usr/bin/env node
/**
 * Run-list probe (v0.4 batch 1d).
 *
 * Checks what the settings → Audit tab renders for the host's runs, and that the
 * two `invoke` names the UI calls are actually registered in the Tauri shell
 * (a typo there fails silently at runtime, which is exactly the kind of thing a
 * probe should catch).
 *
 * The rendering rules live in `ui/src/lib/runView.ts` — a module whose one import is
 * the dependency-free i18n registry — so this imports the **shipped** code directly
 * (Node strips the types). The four v0.6 diff strings are registry keys as of
 * v0.7 batch 1, so they are matched through the shipped `t()` rather than a literal.
 *
 *   node ui/scripts/probe-ui-runs.mjs
 */
import { readFileSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const MODULE = path.join(REPO, "ui", "src", "lib", "runView.ts");
const I18N = path.join(REPO, "ui", "src", "i18n", "index.ts");
const TAB = path.join(REPO, "ui", "src", "settings", "AuditTab.tsx");
const API = path.join(REPO, "ui", "src", "api", "tauri.ts");
const SHELL = path.join(REPO, "ui", "src-tauri", "src", "lib.rs");
const STORE = path.join(REPO, "ui", "src", "state", "appStore.ts");
const HOST_CMDS = path.join(REPO, "host", "src", "commands.rs");

let failures = 0;

function check(name, ok, detail) {
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `: ${detail}` : ""}`);
}

const runs = await import(pathToFileURL(MODULE).href);
const i18n = await import(pathToFileURL(I18N).href);
// The Chinese assertions below are the labels the app ships in its other language;
// the registry's default is English, so pin it (v0.8 batch 2, when these strings
// moved into the registry).
i18n.setLanguage("zh");
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

// Field-level fingerprint diff (v0.6 batch 1, golden-path step 8).
//
// The host returns the rows in the fingerprint's declaration order, and the panel
// renders exactly that: the probe pins the presentation the panel owes the
// operator — a complete list, a header that counts, and no truncation.
//
// Since v0.7 batch 1 the four strings this block renders are i18n registry keys, so
// each assertion below matches the key through the shipped `t()` instead of the old
// Chinese literal: it stays red if `runView` stops reading the registry, whatever
// the machine's locale is (the probe pins the language itself).
const diffRows = [
  { field: "schema", a: { app_version: "0.6.0" }, b: { app_version: "0.6.0" }, is_different: false },
  { field: "llm", a: { model: "deepseek-chat" }, b: { model: "deepseek-reasoner" }, is_different: true },
  { field: "vm", a: { memory_mb: 256 }, b: null, is_different: true },
];
i18n.setLanguage("en");
const diffHeadlineEn = runs.diffTitle(diffRows);
check(
  "the header counts the fields and the differences",
  diffHeadlineEn === i18n.t("diff.headline", { fields: 3, different: 2 }),
  diffHeadlineEn,
);
check(
  "an all-equal diff still counts every field",
  runs.diffTitle(diffRows.map((row) => ({ ...row, is_different: false }))) ===
    i18n.t("diff.headline", { fields: 3, different: 0 }),
  runs.diffTitle(diffRows.map((row) => ({ ...row, is_different: false }))),
);
i18n.setLanguage("zh");
const diffHeadlineZh = runs.diffTitle(diffRows);
check(
  "the header follows the registry's language, not a hard-coded literal",
  diffHeadlineZh !== diffHeadlineEn &&
    diffHeadlineZh === i18n.t("diff.headline", { fields: 3, different: 2 }),
  `${diffHeadlineEn} / ${diffHeadlineZh}`,
);
i18n.setLanguage("en");
check(
  "a changed field is marked and an equal one is not",
  runs.diffRowState(diffRows[1]) === "different" && runs.diffRowState(diffRows[0]) === "same",
);
check("a string value reads as it is", runs.valueText("deepseek-chat") === "deepseek-chat");
check(
  "a nested value is shown whole",
  runs.valueText({ memory_mb: 256 }) === '{\n  "memory_mb": 256\n}',
  runs.valueText({ memory_mb: 256 }),
);
check("a value one run does not carry reads as null", runs.valueText(null) === "null");
check(
  "the loading line comes from the registry",
  runs.diffLoadingText() === i18n.t("diff.loading"),
  runs.diffLoadingText(),
);
check(
  "the empty state comes from the registry",
  runs.diffEmptyText() === i18n.t("diff.empty"),
  runs.diffEmptyText(),
);
const diffRefusal = runs.diffFailedText("no run run_x in this log");
check(
  "a refusal quotes the host",
  diffRefusal === i18n.t("diff.failed", { reason: "no run run_x in this log" }) &&
    diffRefusal.includes("no run run_x in this log"),
  diffRefusal,
);

// Wiring: the tab uses the shipped rule, and the calls resolve to real commands.
const tab = readFileSync(TAB, "utf8");
const store = readFileSync(STORE, "utf8");check("AuditTab uses the run helpers", /from\s+"\.\.\/lib\/runView"/.test(tab));
check("AuditTab renders the rows", /toRunRows\(store\.runs/.test(tab));
check("AuditTab has an empty state", /runsEmptyText\(\)/.test(tab));

const api = readFileSync(API, "utf8");
const shell = readFileSync(SHELL, "utf8");
for (const [name, command] of [
  ["listRuns", "list_runs"],
  ["getRun", "get_run"],
  ["exportRunAudit", "export_run_audit"],
  ["compareRunFingerprints", "compare_run_fingerprints"],
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

// The audit-failure alert (v0.8): the tab renders the banner and the toggle, the
// store wraps the command, and the command exists on both sides.
check(
  "AuditTab shows the failure banner",
  /auditStatus\?\.failures/.test(tab) && /t\("audit\.alert\.heading"\)/.test(tab),
);
check(
  "AuditTab offers the alert toggle",
  /t\("audit\.alert\.toggle"\)/.test(tab) && /store\.setAuditAlert\(/.test(tab),
);
check(
  "AuditTab pops a dialog only under the setting",
  /alertOn && failures\.length > 0/.test(tab) && /from "@tauri-apps\/plugin-dialog"/.test(tab),
);
check("the store wraps set_audit_alert", /api\.setAuditAlert\(/.test(store));
check("the wrapper calls `set_audit_alert`", api.includes('"set_audit_alert"'));
check("`set_audit_alert` is registered in the shell", shell.includes("commands::set_audit_alert"));
check(
  "`set_audit_alert` exists in the host",
  readFileSync(HOST_CMDS, "utf8").includes("pub async fn set_audit_alert"),
);

// The field-level diff, wired end to end (v0.6 batch 1).
check(
  "the store asks the host for the two selected runs' diff",
  /\.compareRunFingerprints\(runA, runB\)/.test(store) &&
    /selectedRuns\.length !== MAX_COMPARED_RUNS/.test(store),
);
check(
  "the tab renders the diff area under the two columns",
  /<details className="diff-block">/.test(tab) &&
    /diffTitle\(store\.diffRows\)/.test(tab) &&
    /valueText\(row\.a\)/.test(tab) &&
    /valueText\(row\.b\)/.test(tab),
);
check("the diff area is collapsed by default", /<details/.test(tab) && !/<details open/.test(tab));
check(
  "the rows keep the order the host sent",
  /store\.diffRows\.map\(/.test(tab) && !/diffRows[\s\S]{0,200}?\.sort\(/.test(tab),
);
check("the placeholder for an unimplemented diff is gone", !/字段级差异是 v0\.6/.test(tab));

// Run-row layout (v0.5 batch 11).
//
// The walkthrough found the audit tab's run rows sitting behind a horizontal
// scrollbar, with the 导出 button off-view: the settings pane was sized by its
// content, and the row is a flex line whose text could not shrink. The contract
// below is what keeps status + fingerprint + button on screen; it is CSS, so the
// probe pins the rules rather than a rendered result.
const LAYOUT = path.join(REPO, "ui", "src", "styles", "layout.css");
const PANELS = path.join(REPO, "ui", "src", "styles", "panels.css");
const layoutCss = readFileSync(LAYOUT, "utf8");
const panelsCss = readFileSync(PANELS, "utf8");
check(
  "the settings pane fills the window",
  /\.settings-page\s*>\s*\.panel\s*\{[^}]*flex:\s*1 1 auto/.test(layoutCss),
  "without it the pane is content-sized and the run rows spill out",
);
check(
  "the run list never scrolls sideways",
  /\.audit-list\s*\{[^}]*overflow-x:\s*hidden/.test(panelsCss),
);
check(
  "run text shrinks with an ellipsis",
  /\.audit-list li > \.action,[\s\S]*?text-overflow:\s*ellipsis/.test(panelsCss),
);
check(
  "row controls never shrink",
  /\.audit-list li > \.badge,[\s\S]*?flex:\s*0 0 auto/.test(panelsCss),
  "the 导出 button must never be the thing that falls off the edge",
);
check(
  "the row's actions sit at the right edge",
  /\.audit-list li \.spacer\s*\{[^}]*flex:\s*1 1 auto/.test(panelsCss),
);
check(
  "the run row keeps the full run id as a tooltip",
  /title=\{r\.runId\}/.test(tab),
);
// The root cause: the global `input { width: 100% }` rule stretched the checkbox,
// which pushed the whole row out of view.
check(
  "the run checkbox has its own size",
  /\.run-pick\s*\{[^}]*width:\s*\d+px/.test(panelsCss),
  "without an explicit width the global input rule makes it fill the row",
);

// The field-level diff's presentation (v0.6 batch 1).
check(
  "a differing field is highlighted",
  /\.diff-row\.different\s*\{[^}]*border-left-color:\s*var\(--accent\)/.test(panelsCss),
);
check(
  "an equal field is dimmed",
  /\.diff-row\.same\s*\{[^}]*color:\s*var\(--muted\)/.test(panelsCss),
);
check("diff values are monospace", /\.diff-value\s*\{[^}]*ui-monospace/.test(panelsCss));
check(
  "diff values wrap instead of being cut off",
  /\.diff-value\s*\{[^}]*white-space:\s*pre-wrap/.test(panelsCss) &&
    /\.diff-value\s*\{[^}]*overflow-wrap:\s*anywhere/.test(panelsCss),
);

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
