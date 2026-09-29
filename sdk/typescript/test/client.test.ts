/**
 * The client and the stream, over a loopback HTTP server — no network beyond `127.0.0.1`, and the
 * requests the client sent come back for inspection.
 */

import { strict as assert } from "node:assert";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import test, { type TestContext } from "node:test";

import {
  Client,
  ClientError,
  frameEnvelope,
  frameKind,
  lostAfter,
  type Envelope,
} from "../src/index.ts";

interface Request {
  method: string;
  url: string;
  headers: Record<string, string | string[] | undefined>;
  body: string;
}

interface Served {
  url: string;
  request: Promise<Request>;
  close: () => Promise<void>;
}

/** One loopback answer, with the request it received. The caller registers `close` as a cleanup. */
async function serve(
  context: TestContext,
  status: number,
  contentType: string,
  body: string,
): Promise<Served> {
  let resolveRequest!: (request: Request) => void;
  const request = new Promise<Request>((resolve) => {
    resolveRequest = resolve;
  });
  const handler = (incoming: IncomingMessage, response: ServerResponse): void => {
    let received = "";
    incoming.on("data", (chunk: Buffer) => {
      received += chunk.toString();
    });
    incoming.on("end", () => {
      resolveRequest({
        method: incoming.method ?? "",
        url: incoming.url ?? "",
        headers: incoming.headers,
        body: received,
      });
      response.writeHead(status, { "Content-Type": contentType });
      response.end(body);
    });
  };
  const server = createServer(handler);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  const port = typeof address === "object" && address !== null ? address.port : 0;
  const close = (): Promise<void> =>
    new Promise<void>((resolve, reject) => {
      // `fetch` keeps the socket alive, so a bare `close()` would wait for it forever — and a test
      // that fails before its own cleanup must not leave the runner hanging either.
      server.closeAllConnections();
      server.close((error) => (error ? reject(error) : resolve()));
    });
  context.after(close);
  return { url: `http://127.0.0.1:${port}`, request, close };
}

test("a query carries the bearer and reads JSON", async (t) => {
  const served = await serve(t, 200, "application/json", '{"status":"ok","uptime_ms":5}');
  const client = new Client(served.url, "the-node-token");

  const answer = (await client.audit_status()) as { status: string };
  assert.equal(answer.status, "ok");

  const request = await served.request;
  assert.equal(request.method, "GET");
  assert.equal(request.url, "/v0/audit/status");
  assert.equal(request.headers["authorization"], "Bearer the-node-token");
});

test("a control posts its JSON body with the bearer and the agent", async (t) => {
  const served = await serve(t, 200, "application/json", '{"id":"req-1"}');
  const client = new Client(served.url, "the-node-token", { agentName: "supervisor-1" });

  const answer = (await client.sandboxes_switch({ name: "blink" })) as { id: string };
  assert.equal(answer.id, "req-1");

  const request = await served.request;
  assert.equal(request.method, "POST");
  assert.equal(request.url, "/v0/sandboxes/switch");
  assert.equal(request.headers["content-type"], "application/json");
  assert.equal(request.headers["x-riscdom-agent"], "supervisor-1");
  assert.equal(request.body, '{"name":"blink"}');
});

test("a bodyless control sends no content type", async (t) => {
  const served = await serve(t, 200, "application/json", "null");
  const client = new Client(served.url, "t");

  await client.vm_stop();

  const request = await served.request;
  assert.equal(request.url, "/v0/vm/stop");
  assert.equal(request.headers["content-type"], undefined);
});

test("query parameters travel in the url", async (t) => {
  const served = await serve(t, 200, "application/json", "[]");
  const client = new Client(served.url, "t");

  await client.workspace_file({ path: "src/lib.rs" });

  const request = await served.request;
  assert.equal(request.url, "/v0/workspace/file?path=src%2Flib.rs");
});

test("an error answer becomes the typed error", async (t) => {
  const body =
    '{"code":"forbidden","message":"the actor may not session.write","retryable":false,"cause":"capability"}';
  const served = await serve(t, 403, "application/json", body);
  const client = new Client(served.url, "t");

  await assert.rejects(
    () => client.audit_status(),
    (error: unknown) => {
      assert.ok(error instanceof ClientError);
      assert.equal(error.kind, "api");
      assert.equal(error.api?.code, "forbidden");
      assert.equal(error.api?.status, 403);
      assert.equal(error.api?.retryable, false);
      assert.equal(error.api?.cause, "capability");
      return true;
    },
  );
});

test("an unreadable error body is still an error", async (t) => {
  const served = await serve(t, 500, "text/html", "<html>not json</html>");
  const client = new Client(served.url, "t");

  await assert.rejects(
    () => client.audit_status(),
    (error: unknown) => {
      assert.ok(error instanceof ClientError);
      assert.equal(error.api?.code, "internal");
      assert.equal(error.api?.cause, null);
      return true;
    },
  );
});

test("workspace_export answers with bytes, not JSON", async (t) => {
  const served = await serve(t, 200, "application/zip", "PK-an-archive");
  const client = new Client(served.url, "t");

  const archive = await client.workspace_export();
  assert.equal(new TextDecoder().decode(archive), "PK-an-archive");

  const request = await served.request;
  assert.equal(request.url, "/v0/workspace/export");
});

test("workspace_import sends the archive and asks for force", async (t) => {
  const served = await serve(t, 200, "application/json", '{"files":1,"bytes":4}');
  const client = new Client(served.url, "t");

  const answer = (await client.workspace_import(new Uint8Array([0x50, 0x4b]), true)) as {
    files: number;
  };
  assert.equal(answer.files, 1);

  const request = await served.request;
  assert.equal(request.url, "/v0/workspace/import?force=true");
  assert.equal(request.headers["content-type"], "application/octet-stream");
});

/**
 * The stream, including the two things the events document insists on: the keep-alive comment is not a
 * frame, and a `gap` is a **re-sync instruction**, surfaced as a kind — never an error, and never
 * abstracted away.
 */
test("frames arrive typed and a gap is an instruction", async (t) => {
  const body = [
    "id: 0-0",
    'data: {"version":1,"kind":"hello","event":null,"agent_id":"server","task_id":null,"ts":1,"payload":{"buffer":{"from":0,"to":42}}}',
    "",
    ": keep-alive",
    "",
    "id: 1758533001207-1",
    'data: {"version":1,"kind":"event","event":"agent:tool_call","agent_id":"dev-1-1","task_id":"task-1-1","ts":1758533001207,"payload":{"name":"write_source"}}',
    "",
    "id: 1758533001880-2",
    'data: {"version":1,"kind":"gap","event":null,"agent_id":"server","task_id":null,"ts":1758533001880,"payload":{"lost_after":"1758533001777-9"}}',
    "",
    "",
  ].join("\n");
  const served = await serve(t, 200, "text/event-stream", body);
  const client = new Client(served.url, "t");

  const subscription = await client.subscribe({ events: ["agent:tool_call"] }, "1758533001000-9");
  t.after(() => subscription.close());
  assert.equal(subscription.lastId, "1758533001000-9");

  const frames = [];
  for await (const frame of subscription) {
    frames.push(frame);
  }
  assert.equal(frames.length, 3, "the keep-alive comment is not a frame");

  assert.equal(frames[0].id, "0-0");
  assert.equal(frameKind(frameEnvelope(frames[0]) as Envelope), "hello");

  const event = frameEnvelope(frames[1]) as Envelope;
  assert.equal(frameKind(event), "event");
  assert.equal(event.event, "agent:tool_call");
  assert.equal(event.task_id, "task-1-1");
  assert.equal(subscription.lastId, "1758533001880-2");

  const gap = frameEnvelope(frames[2]) as Envelope;
  assert.equal(frameKind(gap), "gap");
  assert.equal(lostAfter(gap), "1758533001777-9");

  const request = await served.request;
  assert.equal(request.url, "/v0/events?event=agent%3Atool_call");
  assert.equal(request.headers["accept"], "text/event-stream");
  assert.equal(request.headers["last-event-id"], "1758533001000-9");
});

test("an unknown frame kind is not an error", () => {
  const envelope = frameEnvelope({
    id: null,
    data: '{"version":1,"kind":"something-new","event":null,"agent_id":"server","task_id":null,"ts":1,"payload":{}}',
  });
  assert.equal(frameKind(envelope as Envelope), "unknown");
});
