#!/usr/bin/env node
/**
 * File-picker probe (v0.4 batch 2 — `tauri-plugin-dialog`).
 *
 * Checks what the picker does with a dialog result, and that the plugin is wired
 * end to end: declared in both manifests, initialised in the shell, granted only
 * the permission it needs, and used by the settings tab **without** losing the
 * manual text entry.
 *
 * The decisions live in `ui/src/lib/pathPick.ts` — a dependency-free module — so
 * this imports the shipped code directly (Node strips the types).
 *
 *   node ui/scripts/probe-ui-dialog.mjs
 */
import { readFileSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const MODULE = path.join(REPO, "ui", "src", "lib", "pathPick.ts");
const STORE = path.join(REPO, "ui", "src", "state", "appStore.ts");
const TAB = path.join(REPO, "ui", "src", "settings", "ToolchainTab.tsx");
const PKG = path.join(REPO, "ui", "package.json");
const CARGO = path.join(REPO, "ui", "src-tauri", "Cargo.toml");
const SHELL = path.join(REPO, "ui", "src-tauri", "src", "lib.rs");
const CAPS = path.join(REPO, "ui", "src-tauri", "capabilities", "default.json");

let failures = 0;
function check(name, ok, detail) {
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `: ${detail}` : ""}`);
}

const pick = await import(pathToFileURL(MODULE).href);
console.log(`# path picker (${path.relative(REPO, MODULE)})\n`);

check("a picked path is used", pick.pickedPath("C:\\qemu\\qemu-system-riscv64.exe") === "C:\\qemu\\qemu-system-riscv64.exe");
check("surrounding space is trimmed", pick.pickedPath("  /usr/bin/qemu  ") === "/usr/bin/qemu");
check("a cancelled dialog sets nothing", pick.pickedPath(null) === null);
check("an empty selection sets nothing", pick.pickedPath([]) === null && pick.pickedPath("") === null && pick.pickedPath("   ") === null);
check("several selections collapse to the first", pick.pickedPath(["a.exe", "b.exe"]) === "a.exe");

const win = pick.executableFilters("RISC-V GCC", "Windows NT 10.0; Win64; x64");
const mac = pick.executableFilters("RISC-V GCC", "Macintosh; Intel Mac OS X 10_15_7");
const linux = pick.executableFilters("RISC-V GCC", "X11; Linux x86_64");
check("windows filters to executables", win.length === 1 && win[0].extensions.join(",") === "exe", JSON.stringify(win));
check("windows names the filter", win[0].name === "RISC-V GCC", JSON.stringify(win));
check("no filter elsewhere", mac.length === 0 && linux.length === 0);

// The plugin must be declared in both manifests and initialised in the shell.
const pkg = JSON.parse(readFileSync(PKG, "utf8"));
const cargo = readFileSync(CARGO, "utf8");
const shell = readFileSync(SHELL, "utf8");
const caps = JSON.parse(readFileSync(CAPS, "utf8"));
check("the JS plugin is a dependency", Boolean(pkg.dependencies["@tauri-apps/plugin-dialog"]), pkg.dependencies["@tauri-apps/plugin-dialog"]);
check("the Rust plugin is a dependency", /tauri-plugin-dialog\s*=\s*"2"/.test(cargo));
check("the shell initialises the plugin", /tauri_plugin_dialog::init\(\)/.test(shell));
// v0.5 batch 1 granted `dialog:allow-save` for the audit export; v0.8 added
// `dialog:allow-message` for the audit-failure popup. The set is pinned so the
// permission cannot grow quietly: exactly the dialog permissions we use, each
// with a caller, and never the blanket `dialog:default`.
const DIALOG_PERMISSIONS = [
  "dialog:allow-open",
  "dialog:allow-save",
  "dialog:allow-message",
];
const granted = caps.permissions
  .filter((p) => p.startsWith("dialog:"))
  .sort()
  .join(",");
check(
  "exactly the dialog permissions we use are granted",
  granted === [...DIALOG_PERMISSIONS].sort().join(","),
  `${granted} (granted: ${JSON.stringify(caps.permissions)})`,
);
check(
  "no blanket dialog permission",
  !caps.permissions.includes("dialog:default"),
  JSON.stringify(caps.permissions),
);

// The settings tab uses the picker and keeps the manual entry.
const store = readFileSync(STORE, "utf8");
const tab = readFileSync(TAB, "utf8");
check("the store opens a dialog for the pickers", /await open\(\{/.test(store));
check("the store uses the shipped rules", /pickedPath\(/.test(store) && /executableFilters\(/.test(store));
check("a cancelled pick is ignored", /if \(path\) await set(Toolchain|Qemu)Path\(path\)/.test(store.replace(/\n/g, " ")) || /if \(path\) await setToolchainPath\(path\)/.test(store));
const browseButtons = (tab.match(/store\.pick(Toolchain|Qemu)Path\(\)/g) ?? []).length;
check("both paths offer a browse button", browseButtons === 2, `${browseButtons}`);
const manualPrompts = (tab.match(/window\.prompt\(/g) ?? []).length;
check("manual entry is kept for both", manualPrompts === 2, `${manualPrompts}`);

// The save half of the same plugin, used by the audit export (v0.5 batch 1): one
// caller, a default file name, and nothing exported when the dialog is cancelled.
check(
  "the store imports the save dialog",
  /import\s+\{\s*open,\s*save\s*\}\s+from\s+"@tauri-apps\/plugin-dialog"/.test(store),
);
check("the export asks for a path", /await save\(\{/.test(store) && /defaultPath/.test(store));
check("a cancelled save exports nothing", /if \(!target\) return/.test(store));
check("the export calls the run-audit command", /api\.exportRunAudit\(runId, target\)/.test(store));

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
