#!/usr/bin/env node
/**
 * Snapshot-tab probe (v0.5 batch 11).
 *
 * The walkthrough found the name prompt opening with the hard-coded example
 * `snap1`, so pressing Enter gave every snapshot the same name. The default is now
 * derived from the clock; this pins that rule and the tab's use of it.
 *
 * The rule lives in `ui/src/lib/snapshotName.ts` — a dependency-free module — so
 * this imports the **shipped** code directly (Node strips the types).
 *
 *   node ui/scripts/probe-ui-snapshot.mjs
 */
import { readFileSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const MODULE = path.join(REPO, "ui", "src", "lib", "snapshotName.ts");
const TAB = path.join(REPO, "ui", "src", "settings", "SnapshotTab.tsx");

let failures = 0;
function check(name, ok, detail) {
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `: ${detail}` : ""}`);
}

const names = await import(pathToFileURL(MODULE).href);
console.log(`# snapshot naming (${path.relative(REPO, MODULE)})\n`);

// 2026-09-19 16:02 local time, built from local parts so the test is timezone-agnostic.
const when = new Date(2026, 8, 19, 16, 2, 0).getTime();
const name = names.defaultSnapshotName(when);

check("the default name carries the date and time", name === "snap-20260919-1602", name);
check(
  "the default name is legal for the backend",
  /^[A-Za-z0-9_-]{1,64}$/.test(name),
  "letters, digits, - and _ only, at most 64 characters",
);
check(
  "two snapshots a minute apart do not collide",
  names.defaultSnapshotName(when) !== names.defaultSnapshotName(when + 60_000),
);
check(
  "names sort chronologically",
  names.defaultSnapshotName(when) < names.defaultSnapshotName(when + 60_000),
);
check("month and day are zero-padded", names.defaultSnapshotName(new Date(2026, 0, 5, 9, 7).getTime()) === "snap-20260105-0907", names.defaultSnapshotName(new Date(2026, 0, 5, 9, 7).getTime()));

// The tab must use the rule, and must not keep the old example name.
const tab = readFileSync(TAB, "utf8");
check("the tab imports the naming rule", /from\s+"\.\.\/lib\/snapshotName"/.test(tab));
check(
  "the prompt's default comes from the clock",
  /defaultSnapshotName\(Date\.now\(\)\)/.test(tab),
);
check('the old hard-coded "snap1" default is gone', !/window\.prompt\([^)]*"snap1"/.test(tab));
check(
  "the tab still saves what the user typed",
  /store\.saveSnapshot\(name\.trim\(\)\)/.test(tab),
);

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
