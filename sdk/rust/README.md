[中文](README.zh-CN.md) | English

# riscdom-sdk (Rust)

A **typed Rust client for the RiscDom control plane** (v1.0 M7c), as
[docs/sdk.md](../../docs/sdk.md) freezes it: a thin, typed layer over the surface the other documents
already define, which adds **no semantics** of its own. The HTTP surface is
[control-plane-api.md](../../docs/control-plane-api.md) §5, the error model its §4, authentication its §3,
and the stream [control-plane-events.md](../../docs/control-plane-events.md).

**What is here today (batches BA + BB): the whole surface.** The **38 `GET` queries** of §5.1 and the
**36 `POST` controls** of §5.2 as typed methods, the **bearer token**, the
`{code, message, retryable, cause}` **error as a type**, typed request parameters, and the **event
stream** as a blocking, frame-by-frame [`Subscription`].

**The crate links nothing of this workspace's runtime.** It talks HTTP over `reqwest`'s **blocking**
client and never depends on `host-core`, `server` or `net`, so a client on another machine — the SDK's
normal case — needs nothing else. **No async runtime is imposed** ([sdk.md](../../docs/sdk.md) §3): the
stream is read with `std::io::Read` over the blocking response, which needs no async and no extra
feature. Every dependency (`reqwest`, `serde`, `serde_json`, `thiserror`) was already in `Cargo.lock`,
so the crate adds **edges and no package**.

**The endpoint tables cannot drift from the server.** `QUERY_ENDPOINTS` and `CONTROL_ENDPOINTS` are
committed tables of `(tool, method, path, capability)`, and two tests parse the marked blocks of
[`docs/tool-schema-control-plane.md`](../../docs/tool-schema-control-plane.md) with `include_str!` and
assert the two are equal. Those marked blocks are *already* asserted to be exactly the server's own
`ROUTES`, so the chain is **SDK ⇄ tool schema ⇄ server** — no dependency between them, and no second
list to maintain.

## Use

```rust,ignore
use riscdom_sdk::{AgentRun, AuditEvents, Client, Filters, FrameKind, SnapshotName};

let client = Client::new("http://127.0.0.1:7821", "the-node-token")?;

// Queries: §5.1, one typed method per endpoint.
let status = client.audit_status()?;
let events = client.audit_events(&AuditEvents { limit: 50, ..Default::default() })?;

// Controls: §5.2, the same shape.
let outcome = client.agent_run(&AgentRun {
    user_input: "blink".into(),
    sandbox: None,
    instance: None,
})?;
client.snapshots_save(&SnapshotName { name: "after-blink".into() })?;

// Name the client as an agent, so the audit rows it writes say who acted (api §3).
let client = client.with_agent_name("supervisor-1");

// The event stream: blocking, frame by frame.
let mut subscription = client.subscribe(&Filters { events: vec!["agent:tool_call".into()], ..Default::default() }, None)?;
while let Some(frame) = subscription.next_frame()? {
    match frame.frame_kind() {
        Some(FrameKind::Event) => { /* frame.envelope().unwrap().event … */ }
        Some(FrameKind::Gap) => { /* re-sync from a query — see below */ }
        _ => {}
    }
    let cursor = subscription.last_id(); // send this as Last-Event-ID on a reconnect
}
```

- `Client::new(base_url, auth)` — the bearer value is the node's own token (`<data-dir>/token`); it
  lives in the client so a request cannot forget to carry it.
- `Client::get` / `post` / `post_empty` / `post_raw` / `get_raw` — the transport underneath every
  method, for an endpoint this SDK does not wrap: **the surface is the API's, not the SDK's**.
- Two controls do not speak JSON, and their methods say so: `workspace_export()` returns the archive as
  `Vec<u8>`, and `workspace_import(archive, force)` takes the archive as its body (§5.2).
- Answers are `serde_json::Value`: the API document names each response type (`AuditStatusView`, …) but
  does not freeze its fields, and [sdk.md](../../docs/sdk.md) §1 forbids the SDK inventing them.
- Failures are `ClientError`: `Transport` when the request could not be completed, `Api(ApiError)` when
  the server answered with §4's object. `ApiError` carries `status`, `code`, `message`, `retryable` and
  `cause`; `ClientError::api()` gives it back as a type.

## The stream

`Client::subscribe(filters, last_event_id)` returns a [`Subscription`]. `next_frame()` gives one
[`Frame`] at a time (`id` and `data`), and `frame_kind()` reads the envelope's `kind`:

- **`hello`** — the stream opened; the payload describes its buffer and filters.
- **`event`** — one of the twenty events.
- **`gap`** — **the replay cursor was too old; re-sync from a query.** This is a recovery
  **instruction, not an error** ([control-plane-events.md](../../docs/control-plane-events.md) §2), and
  the SDK surfaces it as a kind rather than papering over it: `Envelope::lost_after()` names the oldest
  id the server still holds. A client that ignores it will silently miss events.
- **`Unknown`** — a kind a future server added. The document says a new kind does not bump `version` and
  clients must ignore what they do not know, so this is a value, not a panic.

**Reconnecting.** Keep the last `id` (`Subscription::last_id`) and pass it back as `last_event_id`; the
client sends it as `Last-Event-ID` and the server replays what follows, or answers with a `gap` if the
cursor has fallen out of its buffer. Comment lines (`: keep-alive`, every 15 seconds) are skipped, and
several `data:` lines in one frame are joined with newlines.

## Layout

- `src/lib.rs` — the endpoint tables, the request-parameter types, the error model, the client and its
  query and control methods, the stream types. The drift guards and the loopback-server tests live at
  the bottom.
- `README.md` / `README.zh-CN.md` — this pair.

The `X-RiscDom-Agent` header (api §3) is sent on every call when `with_agent_name` is used; otherwise the
identity is the credential's own. Nothing here decides what a route means, when a retry is safe, or what
a capability grants — those are the documents' business ([docs/sdk.md](../../docs/sdk.md) §1). The
TypeScript SDK is the next batch (BC).
