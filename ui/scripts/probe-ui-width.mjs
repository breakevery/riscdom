#!/usr/bin/env node
/**
 * Two-pane layout regression probe (v0.3.1 #5).
 *
 * The chat column is user-draggable, but the two columns must always fit inside
 * the window: before v0.3.1 a fixed 240..900 bound let a 900 px chat column push
 * the serial column off-screen in a narrow window.
 *
 * The arithmetic lives in `ui/src/lib/chatWidth.ts` — a dependency-free module —
 * so this probe imports the **shipped** code directly (Node strips the types;
 * Node >= 22.6 with `--experimental-strip-types`, default since 23.6).
 * Run by `scripts/gate.sh` / `scripts/gate.ps1`, and by hand:
 *
 *   node ui/scripts/probe-ui-width.mjs            # the shipped policy + wiring
 *   node ui/scripts/probe-ui-width.mjs --pre-fix  # the pre-v0.3.1 arithmetic, for the record
 */
import { readFileSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const SHELL = path.join(REPO, "ui", "src", "layout", "AppShell.tsx");
const MODULE = path.join(REPO, "ui", "src", "lib", "chatWidth.ts");

const MIN_CHAT_W = 240;
const MAX_CHAT_W = 900;
const RESIZER_W = 6;
const MIN_SERIAL_W = 260;

const preFix = process.argv.includes("--pre-fix");
let failures = 0;

function check(name, ok, detail) {
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `: ${detail}` : ""}`);
}

// The arithmetic before v0.3.1: `onPointerMove` clamped only against the
// absolute bounds, never against the container:
//   Math.max(MIN_CHAT_W, Math.min(MAX_CHAT_W, d.startW + delta))
function preFixColumns(containerW, desired) {
  const chatW = Math.max(MIN_CHAT_W, Math.min(MAX_CHAT_W, desired));
  const total = chatW + RESIZER_W + MIN_SERIAL_W;
  return { chatW, serialMin: MIN_SERIAL_W, total, containerW };
}

const containers = [1400, 1100, 900, 800, 700, 600, 520, 480, 400, 380, 366];
const desireds = [240, 380, 900, 5000];

if (preFix) {
  console.log("# layout before v0.3.1 (fixed 240..900 bound, container ignored)\n");
  for (const c of [700, 480]) {
    const r = preFixColumns(c, 900);
    check(
      `window ${c}px`,
      r.total <= r.containerW,
      `chat ${r.chatW} + resizer ${RESIZER_W} + serial ${r.serialMin} = ${r.total} > ${r.containerW} (overflow ${r.total - r.containerW}px)`,
    );
  }
} else {
  const w = await import(pathToFileURL(MODULE).href);
  console.log(`# shipped policy (${path.relative(REPO, MODULE)})\n`);
  let worst = 0;
  for (const c of containers) {
    for (const d of desireds) {
      const chatW = w.clampChatWidth(d, c, MIN_CHAT_W, MAX_CHAT_W);
      const serialMin = w.serialMinWidth(c, chatW);
      const total = chatW + RESIZER_W + serialMin;
      const hardFloor = MIN_CHAT_W + RESIZER_W + w.HARD_MIN_SERIAL_W;
      const allowed = Math.max(c, hardFloor);
      if (total - allowed > worst) worst = total - allowed;
      if (total > allowed) {
        check(`window ${c}px, wanted ${d}px`, false, `columns add up to ${total} > ${allowed}`);
      }
    }
  }
  check(
    "no combination overflows the window",
    worst <= 0,
    `worst overflow ${worst}px over ${containers.length * desireds.length} combinations`,
  );
  check(
    "resizer/minimums match the CSS",
    w.RESIZER_W === RESIZER_W && w.MIN_SERIAL_W === MIN_SERIAL_W,
  );

  const src = readFileSync(SHELL, "utf8");
  check("AppShell imports the policy", /from\s+"\.\.\/lib\/chatWidth"/.test(src));
  check(
    "AppShell no longer clamps only against MAX_CHAT_W",
    !/Math\.min\(MAX_CHAT_W,\s*d\.startW \+ delta\)/.test(src),
  );
}

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
