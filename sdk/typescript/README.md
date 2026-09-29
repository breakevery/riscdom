[中文](README.zh-CN.md) | English

# @riscdom/sdk (TypeScript)

A **typed TypeScript client for the RiscDom control plane** (v1.0 M7d), as
[docs/sdk.md](../../../docs/sdk.md) freezes it: a thin, typed layer over the surface the other documents
already define, which adds **no semantics** of its own. It is the **TypeScript half of the one surface**
the Rust SDK ([`sdk/rust`](../rust/README.md)) carries; the HTTP surface is
[control-plane-api.md](../../../docs/control-plane-api.md) §5, the error model its §4, authentication its
§3, and the stream [control-plane-events.md](../../../docs/control-plane-events.md).

**What is here: the whole surface.** The **37 `GET` queries** of §5.1 and the **36 `POST` controls** of
§5.2 as typed methods, the bearer token, the `{code, message, retryable, cause}` error as a typed
`ClientError`, typed request parameters, and the **event stream** as an async frame-by-frame
`Subscription`.

**One package, browser and Node, with no runtime dependency.** The runtime is `fetch`, a global in every
current browser and in Node ≥18; the package has **no `dependencies` at all** (and no `devDependencies`
either — the tests run on Node's own test runner with type stripping, so nothing has to be installed).
The stream is read with `fetch` and a `ReadableStream` reader — **never `EventSource`**, which cannot set
the `Authorization` header the API requires
([control-plane-events.md](../../../docs/control-plane-events.md) §1). The package needs **Node ≥23.6**,
where importing `.ts` files directly is the default.

**The endpoint tables cannot drift from the server.** `QUERY_ENDPOINTS` and `CONTROL_ENDPOINTS` are
committed tables of `(tool, method, path, capability)`, and two tests read
`docs/tool-schema-control-plane.md` and assert the two are equal; those marked blocks are already
asserted to be exactly the server's own routes, so the chain is **SDK ⇄ tool schema ⇄ server** — the same
guard the Rust SDK keeps, with no dependency between the two.

## Use

```ts
import { Client, frameEnvelope, frameKind, lostAfter, type Envelope } from "@riscdom/sdk";

const client = new Client("http://127.0.0.1:7821", "the-node-token", { agentName: "supervisor-1" });

// Queries: §5.1, one typed method per endpoint, named as the tool-schema document names it.
const status = await client.audit_status();
const events = await client.audit_events({ limit: 50, actor: "local-1-1" });

// Controls: §5.2, the same shape.
const outcome = await client.agent_run({ user_input: "blink" });
await client.snapshots_save({ name: "after-blink" });

// The event stream.
const subscription = await client.subscribe({ events: ["agent:tool_call"] });
for await (const frame of subscription) {
  const envelope = frameEnvelope(frame) as Envelope;
  switch (frameKind(envelope)) {
    case "event":
      // envelope.event names it; envelope.task_id ties it to a dispatched task.
      break;
    case "gap":
      // The replay cursor was too old: re-sync from a query. `lostAfter(envelope)` names the cursor.
      break;
    default:
      break;
  }
  const cursor = subscription.lastId; // send this back as `lastEventId` to resume
}
```

- `new Client(baseUrl, token, options?)` — the token is the node's own (`<data-dir>/token`); it lives in
  the client so a request cannot forget to carry it. `options.fetch` swaps the implementation (a
  wrapper, a test double).
- `get` / `get_raw` / `post` / `post_empty` — the transport underneath every method, for an endpoint the
  SDK does not wrap: **the surface is the API's, not the SDK's**.
- Two controls do not speak JSON, and their methods say so: `workspace_export()` answers with a
  `Uint8Array`, and `workspace_import(archive, force)` takes the archive as its body (§5.2).
- Answers are `unknown`: the API document names each response type but does not freeze its fields, and
  [sdk.md](../../../docs/sdk.md) §1 forbids the SDK inventing them. Narrow what you need.
- Failures are `ClientError` with `kind: "transport" | "api"`; on `"api"`, `error.api` carries `status`,
  `code`, `message`, `retryable` and `cause`.

## The stream

`client.subscribe(filters, lastEventId?)` returns a `Subscription`: `next_frame()` gives one frame at a
time, and the object is an **async iterable**. `frameKind(envelope)` reads the envelope's `kind`:

- **`hello`** — the stream opened; the payload describes its buffer and filters.
- **`event`** — one of the twenty events.
- **`gap`** — **the replay cursor was too old; re-sync from a query.** A recovery **instruction, not an
  error** ([control-plane-events.md](../../../docs/control-plane-events.md) §2), surfaced as a kind
  rather than papered over: `lostAfter(envelope)` names the oldest id the server still holds. A client
  that ignores it will silently miss events.
- **`unknown`** — a kind a future server added. The document says clients must ignore what they do not
  know, so this is a value, not a throw.

**Reconnecting.** Keep `subscription.lastId` and pass it back as `lastEventId`; the client sends it as
`Last-Event-ID` and the server replays what follows, or answers with a `gap` if the cursor has fallen out
of its buffer. Comment lines (`: keep-alive`, every 15 seconds) are skipped, and several `data:` lines in
one frame are joined with newlines.

## Layout and tests

- `src/index.ts` — the endpoint tables, the parameter types, the error model, the client and its query
  and control methods, and the stream types.
- `test/endpoints.test.ts` — the drift guards (both tables against the document).
- `test/client.test.ts` — the client and the stream over a loopback `node:http` server.
- `npm test` — `node --test`, on Node ≥23.6, with **nothing to install**.
- `npm run typecheck` — `tsc --noEmit`, for whoever has TypeScript available; the gate does not run it,
  because the repository's gate installs no Node dependencies.

The `X-RiscDom-Agent` header (api §3) is sent on every call when `agentName` is set. Nothing here decides
what a route means, when a retry is safe, or what a capability grants — those are the documents' business
([docs/sdk.md](../../../docs/sdk.md) §1). The package is `private` and is not published to any registry.
