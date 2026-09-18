#!/usr/bin/env node
/**
 * Preflight probe (v0.4 batch 3 — environment capability preflight).
 *
 * Checks what the settings tab says about a preflight result (including the
 * escape hatch), and that the three commands exist on both sides with matching
 * names.
 *
 * The wording rules live in `ui/src/lib/preflightView.ts` — a dependency-free
 * module — so this imports the shipped code directly (Node strips the types).
 *
 *   node ui/scripts/probe-ui-preflight.mjs
 */
import { readFileSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const MODULE = path.join(REPO, "ui", "src", "lib", "preflightView.ts");
const TAB = path.join(REPO, "ui", "src", "settings", "ToolchainTab.tsx");
const STORE = path.join(REPO, "ui", "src", "state", "appStore.ts");
const API = path.join(REPO, "ui", "src", "api", "tauri.ts");
const SHELL = path.join(REPO, "ui", "src-tauri", "src", "lib.rs");
const HOST_CMDS = path.join(REPO, "host", "src", "commands.rs");

let failures = 0;
function check(name, ok, detail) {
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `: ${detail}` : ""}`);
}

const view = await import(pathToFileURL(MODULE).href);
console.log(`# preflight wording (${path.relative(REPO, MODULE)})\n`);

const ok = {
  ran: true,
  checked: true,
  ok: true,
  rows: [
    { step: "gcc_runs", state: "ok", detail: null },
    { step: "gcc_compiles", state: "ok", detail: null },
    { step: "qemu_runs", state: "ok", detail: null },
    { step: "guest_boots", state: "ok", detail: null },
  ],
  failed_step: null,
  detail: null,
  suggestion: null,
  overridden: false,
};
const failed = {
  ...ok,
  ok: false,
  rows: [
    { step: "gcc_runs", state: "ok", detail: null },
    { step: "gcc_compiles", state: "failed", detail: "gcc: unknown option" },
    { step: "qemu_runs", state: "not_run", detail: null },
    { step: "guest_boots", state: "not_run", detail: null },
  ],
  failed_step: "gcc_compiles",
  detail: "gcc: unknown option",
  suggestion: "换一条路径或一键下载工具链。",
};

check("step labels are human", view.stepLabel("gcc_compiles") === "编译最小 guest");
check("an unknown step is passed through", view.stepLabel("weird") === "weird");
check("row states are human", view.rowStateLabel("ok") === "通过" && view.rowStateLabel("failed") === "失败" && view.rowStateLabel("not_run") === "未跑");

check("unchecked says so", view.preflightHeadline(null).includes("尚未预检"));
check("passing says so", view.preflightHeadline(ok).includes("预检通过"));
check("failing names the step", view.preflightHeadline(failed).includes("编译最小 guest"), view.preflightHeadline(failed));
check("overridden says the choice is recorded", view.preflightHeadline({ ...failed, overridden: true }).includes("已记录"));

check("the escape hatch is offered after a failure", view.shouldOfferOverride(failed));
check("not offered after a pass", !view.shouldOfferOverride(ok));
check("not offered before any check", !view.shouldOfferOverride({ ...failed, checked: false }));
check("not offered twice", !view.shouldOfferOverride({ ...failed, overridden: true }));

check("progress names the running step", view.progressLine("guest_boots", "running").includes("guest 启动并回显 banner"), view.progressLine("guest_boots", "running"));
check("progress reports a failure", view.progressLine("gcc_compiles", "failed").includes("未通过"));
check("the done line summarises", view.progressLine("done", "ok").includes("全部通过") && view.progressLine("done", "failed").includes("未通过"));
check("no step means no line", view.progressLine(null, null) === "");

// Wiring: the tab renders it, the store follows progress, both sides agree.
const tab = readFileSync(TAB, "utf8");
check("the tab uses the shipped rules", /from\s+"\.\.\/lib\/preflightView"/.test(tab));
check("the tab renders the headline", /preflightHeadline\(store\.preflight\)/.test(tab));
check("the tab renders the rows", /stepLabel\(r\.step\)/.test(tab));
check("the tab offers the escape hatch", /shouldOfferOverride\(store\.preflight\)/.test(tab) && /store\.acknowledgePreflight\(\)/.test(tab));
check("the tab can re-run the check", /store\.runPreflight\(\)/.test(tab));

const store = readFileSync(STORE, "utf8");
check("the store follows `preflight:progress`", store.includes('"preflight:progress"'));
check("the store refreshes the cached status on done", /refreshPreflight\(\)/.test(store));

const api = readFileSync(API, "utf8");
const shell = readFileSync(SHELL, "utf8");
const cmds = readFileSync(HOST_CMDS, "utf8");
for (const command of ["preflight_status", "run_preflight", "acknowledge_preflight"]) {
  check(`the wrapper calls \`${command}\``, api.includes(`"${command}"`));
  check(`\`${command}\` is registered in the shell`, shell.includes(`commands::${command}`));
  check(`\`${command}\` exists in the host`, cmds.includes(`pub async fn ${command}`));
}

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
