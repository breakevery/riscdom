/**
 * The drift guards: the SDK's endpoint tables are exactly the documented marked blocks — which are
 * already asserted to be exactly the server's own routes. One list, two checks, no dependency
 * between them.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import test from "node:test";

import { CONTROL_ENDPOINTS, QUERY_ENDPOINTS, type Endpoint } from "../src/index.ts";

const TOOL_SCHEMA = readFileSync(
  new URL("../../../docs/tool-schema-control-plane.md", import.meta.url),
  "utf8",
);

/** The rows of one marked block, as `[tool, method, path, capability]`. */
function markedRows(block: string): string[][] {
  const begin = `<!-- tool-routes:${block}:begin -->`;
  const end = `<!-- tool-routes:${block}:end -->`;
  let inside = false;
  const rows: string[][] = [];
  for (const line of TOOL_SCHEMA.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === begin) {
      inside = true;
      continue;
    }
    if (trimmed === end) {
      break;
    }
    if (!inside) {
      continue;
    }
    const cells = trimmed.replace(/^\|/, "").replace(/\|$/, "").split("|").map((cell) => cell.trim());
    // The header and the separator are not rows.
    if (cells.length < 5 || !cells[0].startsWith("`")) {
      continue;
    }
    rows.push([
      cells[0].replaceAll("`", ""),
      cells[1],
      cells[2].replaceAll("`", ""),
      cells[3].replaceAll("`", ""),
    ]);
  }
  return rows;
}

function sdkRows(endpoints: readonly Endpoint[]): string[][] {
  return endpoints.map((endpoint) => [
    endpoint.tool,
    endpoint.method,
    endpoint.path,
    endpoint.capability,
  ]);
}

function sorted(rows: string[][]): string[][] {
  return [...rows].sort((left, right) => left.join("\u0000").localeCompare(right.join("\u0000")));
}

test("the endpoint table matches the documented queries", () => {
  assert.deepEqual(sorted(sdkRows(QUERY_ENDPOINTS)), sorted(markedRows("queries")));
  assert.equal(QUERY_ENDPOINTS.length, 37, "§5.1 is 37 queries");
  assert.ok(QUERY_ENDPOINTS.every((endpoint) => endpoint.method === "GET"));
});

test("the control table matches the documented controls", () => {
  assert.deepEqual(sorted(sdkRows(CONTROL_ENDPOINTS)), sorted(markedRows("controls")));
  assert.equal(CONTROL_ENDPOINTS.length, 36, "§5.2 is 36 controls");
  assert.ok(CONTROL_ENDPOINTS.every((endpoint) => endpoint.method === "POST"));
});

test("no two endpoints share a tool name or a method and a path", () => {
  const all = [...QUERY_ENDPOINTS, ...CONTROL_ENDPOINTS];
  for (const [index, endpoint] of all.entries()) {
    for (const other of all.slice(index + 1)) {
      assert.notEqual(endpoint.tool, other.tool, "two endpoints share a tool name");
      assert.ok(
        endpoint.method !== other.method || endpoint.path !== other.path,
        `two endpoints share a method and a path: ${endpoint.method} ${endpoint.path}`,
      );
    }
  }
});
