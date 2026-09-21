#!/usr/bin/env node
/**
 * UI string registry check (v0.7 batch 1).
 *
 * The self-built i18n facility keeps its strings in `ui/src/i18n/strings.ts`: one
 * table per language, keyed semantically. The failure mode this guard exists for is
 * the quiet one — a key added to English and forgotten everywhere else, or a value
 * left empty — because the interface then shows `undefined` or a blank in exactly
 * one language, on exactly the machine nobody is looking at. So the rule is the
 * obvious one, made explicit:
 *
 *   - every language the registry declares has a table;
 *   - every key exists in **every** language (no orphan, no gap);
 *   - no value is empty or whitespace-only;
 *   - a table is not carried for a language the registry does not declare.
 *
 *   node scripts/check-ui-strings.mjs                # self-test, then check this repo
 *   node scripts/check-ui-strings.mjs --self-test    # only the self-test
 *   node scripts/check-ui-strings.mjs --root <dir>   # check another checkout
 *
 * The self-test plants a complete registry and three broken ones and requires the
 * right answers — a guard nobody verifies is a guard that quietly stops working
 * (the same habit `check-wix-version.mjs` follows). The registry is imported by
 * path, the way the ui probes import the shipped `.ts` modules: Node strips the
 * types, and the registry has no dependencies of its own.
 */
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");

const REGISTRY = path.join("ui", "src", "i18n", "strings.ts");

/** Apply the rule above to a set of tables. Returns findings (empty means OK). */
export function checkRegistry(tables, languages) {
  const findings = [];
  const present = [];

  for (const language of languages) {
    const table = tables?.[language];
    if (table === undefined || table === null || typeof table !== "object") {
      findings.push(`language "${language}" has no table`);
      continue;
    }
    present.push(language);
  }

  for (const language of Object.keys(tables ?? {})) {
    if (!languages.includes(language)) {
      findings.push(`language "${language}" has a table but is not declared`);
    }
  }

  const keys = new Set();
  for (const language of present) {
    for (const key of Object.keys(tables[language])) keys.add(key);
  }

  for (const language of present) {
    for (const key of [...keys].sort()) {
      if (!Object.prototype.hasOwnProperty.call(tables[language], key)) {
        findings.push(`${language}: "${key}" is missing — it is an orphan in another language`);
        continue;
      }
      const value = tables[language][key];
      if (typeof value !== "string" || value.trim() === "") {
        findings.push(`${language}: "${key}" is empty`);
      }
    }
  }

  return findings;
}

/** Import the registry from `root` and run the rule over it. */
async function check(root) {
  let module;
  try {
    module = await import(pathToFileURL(path.join(root, REGISTRY)).href);
  } catch (error) {
    return { findings: [`${REGISTRY}: cannot import (${error.message})`], languages: [] };
  }
  const languages = Array.isArray(module.LANGUAGES) ? module.LANGUAGES : [];
  if (languages.length === 0) {
    return { findings: [`${REGISTRY}: LANGUAGES is empty or missing`], languages };
  }
  return { findings: checkRegistry(module.STRINGS, languages), languages };
}

/** Plant a complete registry and three broken ones; require the right answers. */
function selfTest() {
  const cases = [
    { name: "a complete registry", tables: { en: { "a.b": "x" }, zh: { "a.b": "y" } }, want: 0 },
    {
      name: "a key the other language does not carry",
      tables: { en: { "a.b": "x" }, zh: { "a.b": "y", "a.c": "z" } },
      want: 1,
    },
    { name: "an empty value", tables: { en: { "a.b": "   " }, zh: { "a.b": "y" } }, want: 1 },
    { name: "a declared language with no table", tables: { en: { "a.b": "x" } }, want: 1 },
  ];
  const languages = ["en", "zh"];

  for (const testCase of cases) {
    const failed = checkRegistry(testCase.tables, languages).length > 0 ? 1 : 0;
    if (failed !== testCase.want) {
      console.log(
        `ui strings self-test: FAILED — "${testCase.name}" gave ${failed}, expected ${testCase.want}`,
      );
      return false;
    }
  }
  console.log(`ui strings self-test: OK (${cases.length} cases)`);
  return true;
}

function report(result) {
  if (result.findings.length === 0) {
    console.log(`ui strings: OK (${result.languages.length} languages, keys complete)`);
    return 0;
  }
  for (const finding of result.findings) console.log(`  ${finding}`);
  console.log(`ui strings: FAILED (${result.findings.length} finding(s))`);
  return 1;
}

const args = process.argv.slice(2);
if (args.includes("--self-test")) {
  process.exit(selfTest() ? 0 : 1);
}

const rootFlag = args.indexOf("--root");
const explicitRoot = rootFlag === -1 ? null : args[rootFlag + 1];
if (rootFlag !== -1 && !explicitRoot) {
  console.error("usage: node scripts/check-ui-strings.mjs [--self-test] [--root <dir>]");
  process.exit(2);
}

if (!explicitRoot && !selfTest()) {
  process.exit(1);
}
process.exit(report(await check(explicitRoot ?? REPO)));
