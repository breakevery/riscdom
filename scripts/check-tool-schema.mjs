#!/usr/bin/env node
/**
 * Tool-schema guard (v0.9 interface E2).
 *
 * Two documents are deliverables:
 *
 *   docs/tool-schema-executor.md        the eight tools an executor's model is offered
 *   docs/tool-schema-control-plane.md   every endpoint, as a tool for an AI supervisor
 *
 * Both are hand-written, and both are meant to be copied into a `tools[]` array, so a
 * silent drift between a document, its translation and the tables inside it is a lie told
 * to whoever reads it. This script is the document-side half of that guard:
 *
 *   - the marked blocks (`<!-- tool-routes:NAME:begin -->` … `:end`) must be identical in
 *     the English file and its translation;
 *   - every tool name in a marked table must be the derivation of §2 of the document from
 *     that row's method and path (with the six `_post` suffixes and the four verb-named
 *     path-parameter routes it documents);
 *   - every name in a table must appear in the document's own definitions, and every name
 *     in the definitions must be in a table (no orphan either way);
 *   - no two tools may share a name.
 *
 * The code-side half lives in the tests that can see the code: `docs/tool-schema-executor.md`
 * against `agent::tools::tools_json()` (`agent/tests/tool_schema_doc.rs`), and the marked
 * route tables against the server's own `ROUTES` / `LOCAL_ROUTES` / `resolve`
 * (`server/src/routes.rs`'s tests). Neither half repeats the other's job.
 *
 *   node scripts/check-tool-schema.mjs                    # self-test, then check the docs
 *   node scripts/check-tool-schema.mjs --self-test        # only the self-test
 *   node scripts/check-tool-schema.mjs --root <dir>       # check another checkout
 *
 * The self-test copies the documents into a temporary directory, plants a drift in each
 * kind of check, and requires the checker to reject it — a guard nobody verifies is a
 * guard that quietly stops working.
 */
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));

/** The two documents, each as an English/translation pair. */
const PAIRS = [
  ["docs/tool-schema-control-plane.md", "docs/tool-schema-control-plane.zh-CN.md"],
  ["docs/tool-schema-executor.md", "docs/tool-schema-executor.zh-CN.md"],
];

/** The derivation's exceptions, exactly as the document's §2 states them. */
const NAMED_PATTERN_ROUTES = {
  "GET /v0/runs/{run_id}": "run_get",
  "GET /v0/sandboxes/{name}": "sandbox_get",
  "POST /v0/sandboxes/requests/{id}/approve": "sandbox_request_approve",
  "POST /v0/sandboxes/requests/{id}/reject": "sandbox_request_reject",
};

/** The marked blocks of a document: `name -> body`, in file order. */
function markedBlocks(text, marker) {
  const out = new Map();
  let open = null;
  const lines = [];
  for (const line of text.split("\n")) {
    const begin = new RegExp(`^<!-- ${marker}:([a-z-]+):begin -->$`).exec(line.trim());
    if (begin) {
      open = begin[1];
      lines.length = 0;
      continue;
    }
    if (new RegExp(`^<!-- ${marker}:${open}:end -->$`).test(line.trim())) {
      out.set(open, lines.join("\n"));
      open = null;
      continue;
    }
    if (open) lines.push(line);
  }
  return out;
}

/** The fenced ```json block of a document, as text. */
function jsonBlock(text) {
  const lines = text.split("\n");
  const start = lines.findIndex((line) => line.trim() === "```json");
  if (start < 0) return null;
  const end = lines.findIndex((line, index) => index > start && line.trim() === "```");
  if (end < 0) return null;
  return lines.slice(start + 1, end).join("\n");
}

/** §2's rule: the path, folded into a name. */
function deriveName(method, routePath, servedByBoth) {
  const named = NAMED_PATTERN_ROUTES[`${method} ${routePath}`];
  if (named) return named;
  let name = routePath
    .replace(/^\/v0\//, "")
    .replace(/[/\-{}.]/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "");
  if (servedByBoth && method === "POST") name += "_post";
  return name;
}

/** Every `"name":"…"` in a document, in file order. */
function definedNames(text) {
  return [...text.matchAll(/"name"\s*:\s*"([a-z0-9_]+)"/g)].map((match) => match[1]);
}

/**
 * The problems one control-plane document has, as strings.
 *
 * Deliberately self-contained: the same function runs on a real document and on the
 * self-test's mutated copy.
 */
function controlPlaneProblems(text) {
  const problems = [];
  const tables = markedBlocks(text, "tool-routes");
  // Queries, controls, host-local, and the path-parameter routes.
  if (tables.size !== 4) {
    problems.push(`expected 4 marked route tables, found ${tables.size}`);
    return problems;
  }

  const rows = [];
  for (const [block, body] of tables) {
    for (const line of body.split("\n")) {
      const cells = line
        .trim()
        .replace(/^\||\|$/g, "")
        .split("|")
        .map((cell) => cell.trim());
      if (cells.length < 5 || !/^`[a-z0-9_]+`$/.test(cells[0])) continue;
      rows.push({
        block,
        tool: cells[0].replace(/`/g, ""),
        method: cells[1],
        path: cells[2].replace(/`/g, ""),
      });
    }
  }
  if (rows.length === 0) return ["no route rows found in the marked tables"];

  // A path served by both methods gets the `_post` suffix on its POST side.
  const methodsByPath = new Map();
  for (const row of rows) {
    const seen = methodsByPath.get(row.path) ?? new Set();
    seen.add(row.method);
    methodsByPath.set(row.path, seen);
  }

  const seenNames = new Map();
  for (const row of rows) {
    const both = (methodsByPath.get(row.path) ?? new Set()).size > 1;
    const expected = deriveName(row.method, row.path, both);
    if (row.tool !== expected) {
      problems.push(
        `${row.method} ${row.path} is named \`${row.tool}\`, but §2 derives \`${expected}\``,
      );
    }
    const previous = seenNames.get(row.tool);
    if (previous) {
      problems.push(`\`${row.tool}\` is used twice: ${previous} and ${row.method} ${row.path}`);
    } else {
      seenNames.set(row.tool, `${row.method} ${row.path}`);
    }
  }

  const defined = new Set(definedNames(text));
  for (const row of rows) {
    if (!defined.has(row.tool)) {
      problems.push(`\`${row.tool}\` is in a table but has no definition`);
    }
  }
  for (const name of defined) {
    if (!seenNames.has(name)) {
      problems.push(`\`${name}\` has a definition but no table row`);
    }
  }
  return problems;
}

/** The problems one executor document has, as strings. */
function executorProblems(text) {
  const problems = [];
  const block = jsonBlock(text);
  if (block === null) return ["no fenced ```json block"];
  let parsed;
  try {
    parsed = JSON.parse(block);
  } catch (error) {
    return [`the fenced json block is not JSON: ${error.message}`];
  }
  if (!Array.isArray(parsed) || parsed.length === 0) {
    return ["the ```json block must be an array of tools"];
  }
  for (const entry of parsed) {
    const name = entry?.function?.name;
    if (typeof name !== "string" || name.length === 0) {
      problems.push(`a definition has no name: ${JSON.stringify(entry)}`);
      continue;
    }
    if (!text.includes(`| \`${name}\` |`)) {
      problems.push(`\`${name}\` has no row in the human table`);
    }
  }
  return problems;
}

/** The problems a document pair has: the document's own, then the two halves' sameness. */
function pairProblems(root, [english, translated]) {
  const problems = [];
  const read = (file) => readFileSync(path.join(root, file), "utf8");
  let en;
  let zh;
  try {
    en = read(english);
    zh = read(translated);
  } catch (error) {
    return [`${error.message}`];
  }

  const isControlPlane = english.includes("tool-schema-control-plane");
  const check = isControlPlane ? controlPlaneProblems : executorProblems;
  for (const [label, text] of [
    ["en", en],
    ["zh-CN", zh],
  ]) {
    for (const problem of check(text)) {
      problems.push(`${label}: ${problem}`);
    }
  }

  if (isControlPlane) {
    const enTables = markedBlocks(en, "tool-routes");
    const zhTables = markedBlocks(zh, "tool-routes");
    for (const [name, body] of enTables) {
      const other = zhTables.get(name);
      if (other === undefined) {
        problems.push(`zh-CN: the ${name} table is missing`);
      } else if (other !== body) {
        problems.push(`the ${name} tables differ between the languages`);
      }
    }
  } else {
    const enNames = definedNames(en);
    const zhNames = definedNames(zh);
    if (enNames.join(",") !== zhNames.join(",")) {
      problems.push("the ```json blocks differ between the languages");
    }
  }
  return problems;
}

/** Check a checkout; returns the problems found. */
function check(root) {
  const problems = [];
  for (const pair of PAIRS) {
    problems.push(...pairProblems(root, pair));
  }
  return problems;
}

/** The self-test: a planted drift must be rejected, and a clean copy must pass. */
function selfTest() {
    const failures = [];
    const realRoot = path.join(HERE, "..");
    const work = mkdtempSync(path.join(tmpdir(), "riscdom-tool-schema-"));

    // Every case starts from the real documents, copied over whatever the last case
    // mutated: `cpSync` replaces the file, so no case can see another's drift.
    const fresh = () => {
        for (const file of PAIRS.flat()) {
            cpSync(path.join(realRoot, file), path.join(work, file));
        }
    };

    const expect = (label, expected, mutate) => {
        fresh();
        if (mutate) mutate();
        const problems = check(work);
        const ok = expected === "reject" ? problems.length > 0 : problems.length === 0;
        if (!ok) {
            failures.push(
                `${label}: expected the checker to ${expected}, but it found ${
                    problems.length ? problems.join("; ") : "nothing"
                }`,
            );
        }
    };

  const edit = (file, from, to) => {
    const target = path.join(work, file);
    const text = readFileSync(target, "utf8");
    if (!text.includes(from)) {
      failures.push(`self-test: ${file} does not contain ${JSON.stringify(from)}`);
      return;
    }
    writeFileSync(target, text.replace(from, to));
  };

  expect("a clean copy", "accept", null);
  expect("a renamed tool in one language", "reject", () =>
    edit("docs/tool-schema-control-plane.zh-CN.md", "| `tasks` |", "| `dispatch` |"),
  );
  expect("a table row without a definition", "reject", () =>
    edit("docs/tool-schema-control-plane.md", '"name":"tasks"', '"name":"dispatch_renamed"'),
  );
  expect("a translated block that drifted", "reject", () =>
    edit("docs/tool-schema-executor.zh-CN.md", '"name": "compile"', '"name": "compiler"'),
  );
  expect("a table row that is not the derivation", "reject", () =>
    edit("docs/tool-schema-control-plane.md", "| `runs` |", "| `list_runs` |"),
  );

  rmSync(work, { recursive: true, force: true });
  return failures;
}

const args = process.argv.slice(2);
const onlySelfTest = args.includes("--self-test");
const rootAt = args.indexOf("--root");
const root = rootAt >= 0 && args[rootAt + 1] ? path.resolve(args[rootAt + 1]) : path.join(HERE, "..");

let failed = false;

if (!onlySelfTest || args.includes("--self-test")) {
  const failures = selfTest();
  if (failures.length) {
    console.error("tool schema: the self-test failed");
    for (const failure of failures) console.error(`  ${failure}`);
    failed = true;
  } else if (args.includes("--self-test")) {
    console.log("tool schema: self-test OK (a planted drift is rejected)");
  }
}

if (!onlySelfTest) {
  const problems = check(root);
  if (problems.length) {
    console.error("tool schema: FAILED");
    for (const problem of problems) console.error(`  ${problem}`);
    failed = true;
  } else {
    console.log("tool schema: OK (2 documents, both languages, names and tables agree)");
  }
}

process.exit(failed ? 1 : 0);
