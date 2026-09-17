#!/usr/bin/env node
/**
 * Chat-log auto-scroll regression probe (v0.3.1 #4).
 *
 * A run that finishes while the reader has scrolled up in the chat log must not
 * yank them back to the bottom; the panel offers the "jump to latest" button
 * instead, exactly like the serial canvas.
 *
 * The rule itself lives in `ui/src/lib/scrollRule.ts` — a dependency-free module
 * — so this probe can import the **shipped** code directly (Node strips the
 * types; Node >= 22.6 with `--experimental-strip-types`, default since 23.6).
 * Run by `scripts/gate.sh` / `scripts/gate.ps1`, and by hand:
 *
 *   node ui/scripts/probe-ui-scroll.mjs            # the shipped rule + wiring
 *   node ui/scripts/probe-ui-scroll.mjs --pre-fix  # the pre-v0.3.1 rule, for the record
 */
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const PANEL = path.join(REPO, "ui", "src", "panels", "ChatPanel.tsx");
const MODULE = path.join(REPO, "ui", "src", "lib", "scrollRule.ts");

const preFix = process.argv.includes("--pre-fix");
let failures = 0;

function check(name, got, want) {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures += 1;
  console.log(
    `${ok ? "PASS" : "FAIL"}  ${name}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`,
  );
}

// The rule exactly as it was before v0.3.1 (ui/src/panels/ChatPanel.tsx:58-65):
//   useEffect(() => { if (!store.busy) { stickRef.current = true;
//   setShowJump(false); scrollToBottom(); } }, [store.busy])
function preFixOnFinished() {
  return { follow: true, offerJump: false };
}

const scenarios = [
  { name: "reader scrolled up", stick: false, want: { follow: false, offerJump: true } },
  { name: "reader at the bottom", stick: true, want: { follow: true, offerJump: false } },
];

if (preFix) {
  console.log("# rule before v0.3.1 (transcribed from ChatPanel.tsx:58-65)\n");
  for (const s of scenarios) check(s.name, preFixOnFinished(), s.want);
} else {
  const rule = await import(pathToFileURL(MODULE).href);
  console.log(`# shipped rule (${path.relative(REPO, MODULE)})\n`);
  for (const s of scenarios) check(s.name, rule.onRunFinished(s.stick), s.want);

  // The panel must actually use the rule, not keep its own inline copy.
  const src = readFileSync(PANEL, "utf8");
  check("ChatPanel imports the rule", /from\s+"\.\.\/lib\/scrollRule"/.test(src), true);
  check(
    "ChatPanel has no unconditional scrollToBottom() in the busy effect",
    /useEffect\(\(\) => \{\s*if \(!store\.busy\) \{\s*stickRef\.current = true;/.test(src),
    false,
  );
}

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
