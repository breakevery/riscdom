#!/usr/bin/env node
/**
 * Mirror guard (v0.4 1e-followup).
 *
 * Some values belong to `sandbox` / `agent`: the QEMU machine and cpu, the guest
 * RAM, the crt0 injection marker, the RISC-V GCC executable names, the snapshot
 * file extensions. Everything that needs them must reference the exported
 * constant. A second copy inside `host/src` compiles just as well, drifts
 * silently when the owner changes, and quietly makes v0.6's run comparison wrong.
 *
 * This scans `host/src` (production sources only) and fails on a literal copy of
 * any of those values. Comment lines are skipped, because a doc comment may name
 * them.
 *
 *   node scripts/check-mirrored-constants.mjs              # self-test, then scan
 *   node scripts/check-mirrored-constants.mjs --self-test  # only the self-test
 *   node scripts/check-mirrored-constants.mjs --dir <dir>  # scan <dir>, no self-test
 *
 * The self-test plants a literal in a temporary directory and requires the scanner
 * to reject it — a guard nobody verifies is a guard that quietly stops working.
 */
import { mkdtempSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");
const DEFAULT_DIR = path.join(REPO, "host", "src");

/** One rule per mirrored value: the literal to spot, and the owner to point at. */
const RULES = [
  { name: "qemu machine", re: /"virt"/, owner: "sandbox::VM_MACHINE" },
  { name: "qemu cpu", re: /"rv64"/, owner: "sandbox::VM_CPU" },
  { name: "crt0 marker", re: /"injected"/, owner: "agent::CRT0_INJECTED" },
  {
    name: "gcc executable name",
    re: /"riscv64-unknown-elf-gcc"|"riscv-none-elf-gcc"/,
    owner: "agent::GCC_NAMES",
  },
  {
    name: "guest ram",
    re: /const\s+[A-Z][A-Z0-9_]*\s*:\s*u32\s*=\s*128\b/,
    owner: "agent::VM_MEMORY_MB",
  },
  { name: "snapshot extension", re: /\.mig\b/, owner: "sandbox::SNAPSHOT_MIG_EXT" },
];

/** A line whose trimmed text starts a comment is documentation, not code. */
function isComment(line) {
  const trimmed = line.trim();
  return (
    trimmed.startsWith("//") ||
    trimmed.startsWith("/*") ||
    trimmed.startsWith("*") ||
    trimmed.length === 0
  );
}

function rustFiles(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...rustFiles(full));
    } else if (entry.endsWith(".rs")) {
      out.push(full);
    }
  }
  return out;
}

/** All literal copies found under `dir`. */
function scan(dir) {
  const findings = [];
  for (const file of rustFiles(dir)) {
    const lines = readFileSync(file, "utf8").split(/\r?\n/);
    lines.forEach((line, index) => {
      if (isComment(line)) return;
      for (const rule of RULES) {
        if (rule.re.test(line)) {
          findings.push({
            file: path.relative(REPO, file).replace(/\\/g, "/"),
            line: index + 1,
            rule: rule.name,
            owner: rule.owner,
            text: line.trim(),
          });
        }
      }
    });
  }
  return findings;
}

function report(findings, dir) {
  if (findings.length === 0) {
    console.log(`mirrored constants: OK (${rustFiles(dir).length} files in ${path.relative(REPO, dir).replace(/\\/g, "/")})`);
    return 0;
  }
  for (const f of findings) {
    console.log(`  ${f.file}:${f.line}: ${f.rule} — use ${f.owner}`);
    console.log(`      ${f.text}`);
  }
  console.log(`mirrored constants: FAILED (${findings.length} literal copy/copies)`);
  return 1;
}

function selfTest() {
  const dir = mkdtempSync(path.join(tmpdir(), "riscdom-mirror-guard-"));
  writeFileSync(
    path.join(dir, "clean.rs"),
    'let machine = sandbox::VM_MACHINE;\n// "virt" in a comment is documentation\n',
  );
  const clean = scan(dir);
  if (clean.length !== 0) {
    console.log("mirror guard self-test: FAILED — the clean sample was rejected");
    return false;
  }
  writeFileSync(path.join(dir, "planted.rs"), 'const MACHINE: &str = "virt";\n');
  const planted = scan(dir);
  if (planted.length === 0) {
    console.log("mirror guard self-test: FAILED — a planted literal was accepted");
    return false;
  }
  console.log("mirror guard self-test: OK (clean sample passes, planted literal rejected)");
  return true;
}

const args = process.argv.slice(2);
if (args.includes("--self-test")) {
  process.exit(selfTest() ? 0 : 1);
}

const dirFlag = args.indexOf("--dir");
const explicitDir = dirFlag === -1 ? null : args[dirFlag + 1];
if (dirFlag !== -1 && !explicitDir) {
  console.error("usage: node scripts/check-mirrored-constants.mjs [--self-test] [--dir <dir>]");
  process.exit(2);
}

if (!explicitDir && !selfTest()) {
  process.exit(1);
}
process.exit(report(scan(explicitDir ?? DEFAULT_DIR), explicitDir ?? DEFAULT_DIR));
