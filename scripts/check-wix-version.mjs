#!/usr/bin/env node
/**
 * WiX version guard (v0.5 batch 8).
 *
 * A pre-release package version is not a valid MSI `ProductVersion`: WiX takes
 * `major.minor.patch.build`, numeric only, and `tauri-bundler` refuses anything else
 * (`convert_version("1.1.2-alpha")` is an error in its own tests). v0.5.0-preview.1
 * therefore ships with `bundle.windows.wix.version` set to a numeric form, which the
 * bundler uses for the MSI while the artifact names keep the package version.
 *
 * That override is a preview-only crutch, and it is exactly the kind of value that
 * goes silently wrong: once the package version is numeric again, a leftover
 * `wix.version` makes the MSI's ProductVersion disagree with the version every other
 * artifact, tag and document carries — with no build error to notice it by.
 *
 * The rule:
 *   - package version WITHOUT a pre-release suffix  -> `wix.version` must NOT exist
 *   - package version WITH a pre-release suffix     -> `wix.version` may exist, and
 *                                                      must be numeric `x.y.z[.w]`
 *
 *   node scripts/check-wix-version.mjs                # self-test, then check this repo
 *   node scripts/check-wix-version.mjs --self-test    # only the self-test
 *   node scripts/check-wix-version.mjs --root <dir>   # check another checkout
 *
 * The self-test plants both situations in a temporary directory and requires the
 * guard to answer correctly — a guard nobody verifies is a guard that quietly stops
 * working.
 */
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");

const PKG = path.join("ui", "package.json");
const CONF = path.join("ui", "src-tauri", "tauri.conf.json");

/** Is this a semver version whose pre-release part is empty? */
function isNumericVersion(version) {
  return !version.includes("-") && !version.includes("+");
}

/** Read both files and decide. Returns findings (empty means OK). */
function check(root) {
  const findings = [];
  const pkgPath = path.join(root, PKG);
  const confPath = path.join(root, CONF);

  let pkg;
  let conf;
  try {
    pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
  } catch (error) {
    findings.push(`${PKG}: cannot read/parse (${error.message})`);
    return { findings, version: null, wixVersion: null };
  }
  try {
    conf = JSON.parse(readFileSync(confPath, "utf8"));
  } catch (error) {
    findings.push(`${CONF}: cannot read/parse (${error.message})`);
    return { findings, version: pkg.version ?? null, wixVersion: null };
  }

  const version = pkg.version;
  const wixVersion = conf.bundle?.windows?.wix?.version;

  if (typeof version !== "string" || version.length === 0) {
    findings.push(`${PKG}: no "version"`);
    return { findings, version: null, wixVersion: wixVersion ?? null };
  }

  if (wixVersion === undefined) {
    // Nothing to pin: correct for a numeric version, and legal for a pre-release one.
    return { findings, version, wixVersion: null };
  }

  if (typeof wixVersion !== "string" || wixVersion.length === 0) {
    findings.push(`${CONF}: bundle.windows.wix.version must be a non-empty string`);
    return { findings, version, wixVersion: null };
  }

  if (isNumericVersion(version)) {
    findings.push(
      `${CONF}: bundle.windows.wix.version is set to "${wixVersion}" while the package ` +
        `version "${version}" is already numeric — remove the override, or the MSI's ` +
        `ProductVersion silently disagrees with the package version`,
    );
  }

  if (!/^\d+\.\d+\.\d+(\.\d+)?$/.test(wixVersion)) {
    findings.push(
      `${CONF}: bundle.windows.wix.version "${wixVersion}" is not numeric ` +
        `(expected major.minor.patch[.build]) — WiX rejects anything else`,
    );
  }

  return { findings, version, wixVersion };
}

function report(result) {
  if (result.findings.length === 0) {
    const pinned = result.wixVersion === null ? "not pinned" : `pinned to ${result.wixVersion}`;
    console.log(
      `wix version: OK (package ${result.version}, wix.version ${pinned})`,
    );
    return 0;
  }
  for (const finding of result.findings) {
    console.log(`  ${finding}`);
  }
  console.log(`wix version: FAILED (${result.findings.length} finding(s))`);
  return 1;
}

/** Plant both situations in a temp checkout and require the right answers. */
function selfTest() {
  const root = mkdtempSync(path.join(tmpdir(), "riscdom-wix-guard-"));
  mkdirSync(path.join(root, "ui", "src-tauri"), { recursive: true });

  const write = (version, wixVersion) => {
    writeFileSync(
      path.join(root, PKG),
      `${JSON.stringify({ name: "ui", version }, null, 2)}\n`,
    );
    const bundle = { active: true, targets: "all" };
    if (wixVersion !== null) {
      bundle.windows = { wix: { version: wixVersion } };
    }
    writeFileSync(
      path.join(root, CONF),
      `${JSON.stringify({ version, bundle }, null, 2)}\n`,
    );
  };

  const cases = [
    { name: "numeric version + wix override", version: "0.5.0", wix: "0.5.0.1", want: 1 },
    { name: "numeric version, no override", version: "0.5.0", wix: null, want: 0 },
    { name: "pre-release version + override", version: "0.5.0-preview.1", wix: "0.5.0.1", want: 0 },
    { name: "pre-release version, no override", version: "0.5.0-preview.1", wix: null, want: 0 },
    { name: "non-numeric override", version: "0.5.0-preview.1", wix: "0.5.0-preview.1", want: 1 },
  ];

  for (const testCase of cases) {
    write(testCase.version, testCase.wix);
    const result = check(root);
    const failed = result.findings.length > 0 ? 1 : 0;
    if (failed !== testCase.want) {
      console.log(
        `wix guard self-test: FAILED — "${testCase.name}" gave ${failed}, expected ${testCase.want}`,
      );
      for (const finding of result.findings) console.log(`      ${finding}`);
      return false;
    }
  }
  console.log(`wix guard self-test: OK (${cases.length} cases)`);
  return true;
}

const args = process.argv.slice(2);
if (args.includes("--self-test")) {
  process.exit(selfTest() ? 0 : 1);
}

const rootFlag = args.indexOf("--root");
const explicitRoot = rootFlag === -1 ? null : args[rootFlag + 1];
if (rootFlag !== -1 && !explicitRoot) {
  console.error("usage: node scripts/check-wix-version.mjs [--self-test] [--root <dir>]");
  process.exit(2);
}

if (!explicitRoot && !selfTest()) {
  process.exit(1);
}
process.exit(report(check(explicitRoot ?? REPO)));
