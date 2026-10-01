[中文](sdk.zh-CN.md) | English

# The SDKs

**Status** v1.0 specification (M7c / M7d) ｜ **Date** 2026-09-29 ｜ **Audience** people writing a client against
RiscDom, and whoever builds the libraries they use.

**What this document is.** [roadmap §11](roadmap-v1.0.md) puts an **SDK** in v1.0's ecosystem work — *"[default]*
Rust and TypeScript first, because those are the two languages this repository already speaks" ([§14.12](roadmap-v1.0.md)) —
and the SDKs are M7c and M7d. This document says what those libraries **are**: a thin, typed layer over the
surface the other documents already freeze, so a client author does not hand-roll the wire. It is a
**specification**; the libraries are a later batch.

**Two SDKs, one surface.** Rust and TypeScript, per §14.12. They are two views of **the same contract** —
[control-plane-api.md](control-plane-api.md), [control-plane-events.md](control-plane-events.md) and the
configuration types of [config-schema.md](config-schema.md) — not two designs. Anything one SDK can say, the
other says, in its own idiom.

## 1. What an SDK is, and what it is not

**It is a typed client.** It carries:
- the **request and response types** of every endpoint, so a call is a function and not a stringly-typed
  dictionary;
- the **error model** ([control-plane-api.md](control-plane-api.md) §4) as a typed error, not an exception
  with a string in it;
- **authentication** (the bearer token) and the **capability** the call needs, surfaced where it is checked;
- the **event stream** ([control-plane-events.md](control-plane-events.md)) as a typed subscription with the
  envelope's `kind` — `hello`, `event`, `gap` — spelled out.

**It is not a re-implementation.** The SDK adds **no semantics**: it does not decide what a route means, when
a retry is safe, or what a capability grants. Those are settled in the documents above, and the SDK is a
faithful mirror of them. Where a rule lives in the API document, the SDK's documentation **points at that
document** rather than restating it, so there is one place to change.

**It is not a product feature.** The SDK is for **third parties**. This repository's own clients — the CLI
(`cli/src/client.rs`) and the management program's front end (`ui/src/api/`) — predate it and are not
rewritten onto it: they are inside the project, the SDK is for outside.

## 2. The surface it wraps

The SDK covers exactly what the API serves, and nothing the API does not:

| Piece | Its document | What the SDK makes of it |
|---|---|---|
| The HTTP surface | [control-plane-api.md](control-plane-api.md) §5 — the query and control endpoint tables | One typed method per endpoint, named for the endpoint's `(method, path)` |
| Authentication and capabilities | [control-plane-api.md](control-plane-api.md) §3 | The bearer token in configuration; the capability a call needs, named in the method's documentation |
| The error model | [control-plane-api.md](control-plane-api.md) §4 — `{code, message, retryable, cause}` | A typed error carrying all four fields, never a bare string |
| The event stream | [control-plane-events.md](control-plane-events.md) — the envelope and its `kind` | A typed subscription; `gap` is surfaced as a **recovery instruction**, not an error |
| The persisted types | [config-schema.md](config-schema.md) | The configuration types a client reads or writes, from the schema the document freezes |

**The route list is the single source of truth.** [control-plane-api.md](control-plane-api.md) §5's tables are
asserted against the server's own route definition by a test (`the_table_has_the_documented_endpoints`), so an
SDK generated from them cannot drift from the server. That is the point of generating the SDK's surface rather
than hand-copying it: **a new endpoint reaches the SDKs because it reached the table first.**

## 3. The Rust SDK (M7c)

- **A crate that depends on nothing of this workspace's runtime.** It talks HTTP; it never links `host-core`,
  and it never needs the control plane *in-process*. A client on another machine is the SDK's normal case.
- **No runtime imposed.** The library does not force an async executor on a consumer who only wants a blocking
  call; whether it is sync, async, or both is the implementation batch's, and it is decided so that a
  single-threaded tool can use it.
- **The types are the schema's.** What the kernel persists ([config-schema.md](config-schema.md)) and what the
  API answers ([control-plane-api.md](control-plane-api.md) §5) are the SDK's types, not a parallel model.

## 4. The TypeScript SDK (M7d)

- **One package, browser and Node.** It is the same kinds of call the management program's adapter already
  makes over HTTP (`ui/src/api/http.ts`), promoted into a library with the types attached.
- **The stream is read with `fetch`, not `EventSource`.** This is a documented gotcha, not a preference:
  `EventSource` cannot set headers and the stream needs `Authorization: Bearer`, and the frames are
  `id:` / `data:` lines ([control-plane-client-guide.md](control-plane-client-guide.md) §11). The SDK hides
  that rule behind a subscription, but it does not abstract away the `gap` semantics behind it.
- **The types ship.** A consumer binds `Bearer` and the base URL, and every response is typed, so the client
  author does not redeclare the API's shapes.

## 5. Versioning, and why an SDK can promise anything at all

An SDK version is only as meaningful as the API's stability, and **the API is not stable yet**:
[control-plane-api.md](control-plane-api.md) §7 says the whole v0.x line ships breaking changes without a
prefix bump, and **v1.0 is the freeze** (`/v0/` is the path it ships with, additive fields do not bump the
version, and a semantic change does).

So the SDKs follow the API's own rule, and say so out loud:
- **Before the freeze**, an SDK is honest about instability: it pins a **RiscDom version range**, not an API
  version, exactly as §7 prescribes for any v0.x client.
- **At the freeze**, the SDKs inherit the guarantee §7 states — additive fields do not bump, a semantic change
  does — and that is the first version an SDK can call stable.
- **The persisted types follow their markers.** Where the SDK exposes a persisted shape, its compatibility
  rule is [api-compatibility.md §6](api-compatibility.md)'s marker, not the SDK's own numbering.

## 6. One surface, no fourth copy

This repository already has the surface in more than one place, and the SDK must not add a **divergent** one:

- **The API document is the contract** ([control-plane-api.md](control-plane-api.md)); its tables are asserted
  against the server, so they are the truth.
- **The CLI and the front end are consumers, not sources.** `cli/src/client.rs` speaks HTTP and never calls
  the state directly; `ui/src/api/` is one surface with two transports (Tauri IPC and HTTP). Neither is the
  contract, and neither is rewritten onto the SDK — they predate it.
- **The SDKs are generated from the contract where they can be, hand-written where they must be.** The
  endpoint surface and the types come from the documents and their assertions; the ergonomics — a builder, an
  error type, a subscription — are written. The **rule** is that a fact lives in one place: adding a route
  adds it to the API table, and both SDKs follow from that table rather than from a second list.

**Relation to the second repository.** The SDK is a *third-party* library, distinct from the kernel's own
crates; it is not what the management program (now `riscdom-adminapp`,
[multi-repo.md](multi-repo.md)) consumes — the program is inside the project and uses its own adapter. Whether
the SDKs are published to a registry (crates.io, npm) is the implementation batch's, tied to the same question
[multi-repo.md §2](multi-repo.md) defers for the kernel.

## 7. What is not covered

- **No SDK for another language yet.** §14.12 makes Rust and TypeScript the **priority**, not the limit — a
  later language follows the same contract, and the surface it wraps is already language-neutral (HTTP + JSON
  + an SSE-shaped stream).
- **No hosted service, no server-side component.** The SDK is a library a client links; it dials the node the
  client names and nothing else. The project runs no service ([roadmap §1](roadmap-v1.0.md)).
- **No per-capability credentials.** Today the single token holds the whole vocabulary
  ([control-plane-client-guide.md](control-plane-client-guide.md) §10); per-capability tokens are v1.0 work,
  and the SDK will surface them when they exist.
- **No behaviour the API does not have.** The four kernel-capability gaps
  ([control-plane-api.md](control-plane-api.md) §6) are gaps in the API; the SDK reports them as the API
  reports them and does not paper over them.
- **The libraries themselves** — their packaging, their exact module layout and their publication — are the
  implementation batch's, written against this document.
