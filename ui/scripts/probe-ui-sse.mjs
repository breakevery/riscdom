#!/usr/bin/env node
/**
 * The event stream's client probe (v0.9 D2b-3).
 *
 * Two layers, and both are checked here because both can fail quietly:
 *
 * - the **wire format** (`ui/src/lib/sse.ts`) — a pure module, so its rules are
 *   imported and exercised directly: line classification, a frame that has not
 *   finished arriving, several `data:` lines in one frame, the `id:` cursor, a
 *   comment heartbeat, the end of a stream mid-frame;
 * - the **transport** (`ui/src/api/http.ts`) — with `fetch` replaced by a stand-in
 *   that serves a `ReadableStream`, so the subscription, the dispatch by envelope,
 *   the cursor sent back as `Last-Event-ID` and the `gap` callback are all checked
 *   without a server.
 *
 * Node has `ReadableStream`, `TextDecoder` and `AbortController`, so nothing is
 * shimmed and nothing touches a socket.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const SSE = path.join(REPO, "ui", "src", "lib", "sse.ts");
const HTTP = path.join(REPO, "ui", "src", "api", "http.ts");
const GATE = path.join(REPO, "scripts", "gate.sh");

let failures = 0;

function check(name, ok, detail) {
  if (ok) {
    console.log(`PASS  ${name}${detail ? `: ${detail}` : ""}`);
  } else {
    failures += 1;
    console.log(`FAIL  ${name}${detail ? `: ${detail}` : ""}`);
  }
}

const sse = await import(pathToFileURL(SSE).href);

// ----- the wire format --------------------------------------------------------

const lines = [
  ["id: 1-0", "id"],
  ["data: {}", "data"],
  [": keep-alive", "comment"],
  ["", "empty"],
  ["event: something", "other"],
];
for (const [raw, want] of lines) {
  check(`parseSseLine classifies ${JSON.stringify(raw)}`, sse.parseSseLine(raw).kind === want);
}
check(
  "a field's value loses exactly one leading space",
  sse.parseSseLine("data:  two").value === " two" && sse.parseSseLine("data:x").value === "x",
);
check("a trailing CR is tolerated", sse.parseSseLine("data: x\r").value === "x");

const reader = new sse.SseReader();
check(
  "a frame that has not finished arriving produces nothing",
  reader.push("data: {\"half\":") .length === 0,
);
const frames = reader.push('true}\n\n');
check("…and the rest of it completes the frame", frames.length === 1, JSON.stringify(frames));
check("its data is the whole payload", frames[0].data === '{"half":true}');

const multi = new sse.SseReader().push("id: 2-1\ndata: one\ndata: two\n\n");
check(
  "several data: lines in one frame are joined with newlines",
  multi.length === 1 && multi[0].data === "one\ntwo" && multi[0].id === "2-1",
  JSON.stringify(multi),
);

const heartbeat = new sse.SseReader().push(": keep-alive\n\n");
check("a comment heartbeat is not a frame", heartbeat.length === 0);

const cursor = new sse.SseReader();
cursor.push("id: 3-9\n");
const flushed = cursor.flush();
check("an id alone is not a frame at the end of a stream", flushed === undefined);
const tail = new sse.SseReader();
tail.push("id: 4-0\ndata: {\"kind\":\"event\"}");
const tailFrame = tail.flush();
check(
  "a frame the stream ended in the middle of still counts",
  tailFrame !== undefined && tailFrame.data === '{"kind":"event"}' && tailFrame.id === "4-0",
  JSON.stringify(tailFrame),
);
const envelope = JSON.parse(tailFrame.data);
check("its envelope parses like any other", envelope.kind === "event");

// ----- the transport ----------------------------------------------------------

const requests = [];
const streamText = (text) =>
  new ReadableStream({
    start(controller) {
      controller.enqueue(new TextEncoder().encode(text));
      controller.close();
    },
  });

globalThis.fetch = async (url, init) => {
  requests.push({ url, init });
  const body =
    requests.length === 1
      ? "id: 1-0\ndata: {\"version\":1,\"kind\":\"hello\",\"event\":null,\"payload\":{\"buffer\":{\"from\":0,\"to\":1}}}\n\n" +
        ": keep-alive\n\n" +
        'id: 1-1\ndata: {"version":1,"kind":"event","event":"agent:final","payload":{"kind":"final","content":"done","iterations":1}}\n\n'
      : 'id: 1-2\ndata: {"version":1,"kind":"gap","event":null,"payload":{"lost_after":"1-0"}}\n\n';
  return {
    ok: true,
    status: 200,
    body: streamText(body),
  };
};

const http = await import(pathToFileURL(HTTP).href);

const finals = [];
const gaps = [];
http.setToken("a-token");
const offGap = http.onGap(() => gaps.push("gap"));
const offFinal = http.onHostEvent("agent:final", (payload) => finals.push(payload));

// Wait for the first stream to be read, and for the reconnect it schedules.
await new Promise((resolve) => setTimeout(resolve, 1_300));

check("the stream is dialled at /v0/events", requests[0].url === "/v0/events", requests[0].url);
check(
  "the stream asks for events and presents the token",
  requests[0].init.headers.Accept === "text/event-stream" &&
    requests[0].init.headers.Authorization === "Bearer a-token",
  JSON.stringify(requests[0].init.headers),
);
check(
  "an event frame is dispatched to its subscriber, unwrapped",
  finals.length === 1 && finals[0].content === "done" && finals[0].kind === "final",
  JSON.stringify(finals),
);
check(
  "a reconnect resumes from the cursor the frames carried",
  requests.length > 1 && requests[1].init.headers["Last-Event-ID"] === "1-1",
  `${requests.length} request(s); last-event-id=${requests[1]?.init.headers["Last-Event-ID"]}`,
);
check("a gap frame reaches the gap subscribers, not the event ones", gaps.length === 1 && finals.length === 1);

offFinal();
offGap();
await new Promise((resolve) => setTimeout(resolve, 50));

// ----- the gate runs it -------------------------------------------------------

const gate = readFileSync(GATE, "utf8");
check("gate.sh runs this probe", /probe-ui-sse\.mjs/.test(gate));

console.log(`\n${failures === 0 ? "OK" : `${failures} failing check(s)`}`);
process.exit(failures === 0 ? 0 : 1);
