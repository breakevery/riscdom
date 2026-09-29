[中文](README.zh-CN.md) | English

# riscdom-sdk (Rust)

A **typed Rust client for the RiscDom control plane** (v1.0 M7c), as
[docs/sdk.md](../../docs/sdk.md) freezes it: a thin, typed layer over the surface the other documents
already define, which adds **no semantics** of its own. The HTTP surface is
[control-plane-api.md](../../docs/control-plane-api.md) §5, the error model its §4, authentication its §3.

**What is here today (batch BA): the query half.** The **37 `GET` endpoints** of §5.1 as typed methods,
the **bearer token**, the `{code, message, retryable, cause}` **error as a type**, and the request
parameters of the endpoints that take them. **Not yet:** the control endpoints (`POST`, §5.2) and the
event stream, which are the next batch (BB); the TypeScript SDK is BC.

**The crate links nothing of this workspace's runtime.** It talks HTTP over `reqwest`'s **blocking**
client and never depends on `host-core`, `server` or `net`, so a client on another machine — the SDK's
normal case — needs nothing else, and **no async runtime is imposed** ([sdk.md](../../docs/sdk.md) §3).
`reqwest` is already in `Cargo.lock`, so this crate adds an edge and **no new package**.

**The endpoint table cannot drift from the server.** `QUERY_ENDPOINTS` is a committed table of
`(tool, method, path, capability)`, and a test parses the marked blocks of
[`docs/tool-schema-control-plane.md`](../../docs/tool-schema-control-plane.md) with `include_str!` and
asserts the two are equal. Those marked blocks are *already* asserted to be exactly the server's own
`ROUTES`, so the chain is **SDK ⇄ tool schema ⇄ server** — no dependency between them, and no second
list to maintain. The guard is a test, so it needs no build step and no code generation.

## Use

```rust,ignore
use riscdom_sdk::{AuditEvents, Client};

let client = Client::new("http://127.0.0.1:7821", "the-node-token")?;

// One typed method per endpoint, named as the tool-schema document names it.
let status = client.audit_status()?;
let events = client.audit_events(&AuditEvents { limit: 50, ..Default::default() })?;

// Name the client as an agent, so the audit rows it writes say who acted (api §3).
let client = client.with_agent_name("supervisor-1");
```

- `Client::new(base_url, auth)` — the bearer value is the node's own token
  (`<data-dir>/token`); it lives in the client so a request cannot forget to carry it.
- `Client::get(path, query)` — the transport underneath every method, for an endpoint this batch does
  not wrap: **the surface is the API's, not the SDK's**.
- Answers are `serde_json::Value`: the API document names each response type (`AuditStatusView`, …) but
  does not freeze its fields, and [sdk.md](../../docs/sdk.md) §1 forbids the SDK inventing them, so this
  batch does not. Typed response structs follow when a document fixes the shapes.
- Failures are `ClientError`: `Transport` when the request could not be completed, `Api(ApiError)` when
  the server answered with §4's object. `ApiError` carries `status`, `code`, `message`, `retryable` and
  `cause`; `ClientError::api()` gives it back as a type.

## Layout

- `src/lib.rs` — the endpoint table, the request-parameter types, the error model, the client and its 37
  query methods. The drift guard and the loopback-server tests live at the bottom.
- `README.md` / `README.zh-CN.md` — this pair.

The `X-RiscDom-Agent` header (api §3) is sent when `with_agent_name` is used; otherwise the identity is
the credential's own. Nothing here decides what a route means, when a retry is safe, or what a capability
grants — those are the documents' business ([docs/sdk.md](../../docs/sdk.md) §1).
