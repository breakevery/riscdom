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
check("only `dialog:allow-open` is granted", caps.permissions.includes("dialog:allow-open"), JSON.stringify(caps.permissions));
check(
  "no broader dialog permission sneaks in",
  !caps.permissions.some((p) => p === "dialog:default" || p.startsWith("dialog:allow-save")),
  JSON.stringify(caps.permissions),
);

// The settings tab uses the picker and keeps the manual entry.
const store = readFileSync(STORE, "utf8");
const tab = readFileSync(TAB, "utf8");
check("the store calls the dialog plugin", /import\s+\{\s*open\s*\}\s+from\s+"@tauri-apps\/plugin-dialog"/.test(store));
check("the store uses the shipped rules", /pickedPath\(/.test(store) && /executableFilters\(/.test(store));
check("a cancelled pick is ignored", /if \(path\) await set(Toolchain|Qemu)Path\(path\)/.test(store.replace(/\n/g, " ")) || /if \(path\) await setToolchainPath\(path\)/.test(store));
const browseButtons = (tab.match(/store\.pick(Toolchain|Qemu)Path\(\)/g) ?? []).length;
check("both paths offer a browse button", browseButtons === 2, `${browseButtons}`);
const manualPrompts = (tab.match(/window\.prompt\(/g) ?? []).length;
check("manual entry is kept for both", manualPrompts === 2, `${manualPrompts}`);

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
