[中文](error-model.zh-CN.md) | English

# The error model

**Status** v1.0 specification (milestone [M1](roadmap-v1.0.md)) ｜ **Date** 2026-09-27 ｜ **Baseline**
v0.9.9 (`3365970`) ｜ **Audience** kernel developers — with §2 and §8 (the wire) written to be normative
for **distribution integrators** as well.

**Why an error model is part of a freeze.** A caller can only act on a failure if the failure says which
kind it is. "Something went wrong" makes every client either retry everything or retry nothing, and both
are wrong: retrying a refusal wastes the network, and not retrying a timeout loses the work. This
document is the third of the six things the [v1.0 roadmap](roadmap-v1.0.md) §6 says must be on disk
before the API may be called frozen.

**What is already true.** The HTTP envelope, its four fields and the closed-for-v0.9 list of `code`s are
normative in [`control-plane-api.md`](control-plane-api.md) §4. This document does not replace that
section: it **extends** it — the same field names, a longer list of causes, and the same mapping rule
(§8) — and it adds the in-process half, `DispatchError`, in the shape [decisions §12](decisions.md)
already fixed (categories, a `Retryable` flag, a `Cause` chain, and serialisability across processes and
devices).

## 1. Two layers, one vocabulary

- **[settled]** **In process**: `DispatchError`, the type a dispatcher returns when a task produced no
  outcome ([`agent/src/dispatch.rs`](../agent/src/dispatch.rs)). It is `serde`-serialisable, because a
  worker process writes it back as one JSON line.
- **[settled]** **On the wire**: one error object per non-2xx response, `{code, message, retryable,
  cause}`. It is what a client that is not this program sees.
- **[settled]** **The wire layer is the kernel's answer, not the dispatch's echo.** A `DispatchError`
  becomes a `code` plus a status (§8) at the boundary; the two vocabularies are related by that mapping
  and are not the same enum.

## 2. The wire layer

**[settled]** Today's object, unchanged in shape:

```json
{
  "code": "not_found",
  "message": "no run with id run-12345-7",
  "retryable": false,
  "cause": "run_id"
}
```

| Field | Type (v0.x) | Type (from the next prefix move) | Meaning |
|---|---|---|---|
| `code` | string | string | Stable and machine-readable. The list is **closed for v0.9**; v1.0 extends it (§8) and then closes it again. |
| `message` | string | string | Human-readable. **Never contains a secret** — no token, no key, no credential. |
| `retryable` | bool | bool | Whether an identical retry can plausibly succeed (§4). |
| `cause` | string \| null | **string[]** | The offending input field or subsystem. The next prefix move makes it a **chain**: outermost first, so `["network", "executor"]` reads "the network, and under it the executor". |

- **[settled]** **`cause` becomes a chain at the next prefix move, not at v1.0.** v1.0 freezes the **paths**
  (`/v0/` is the path it ships with, [`api-compatibility.md`](api-compatibility.md) §5), not this field: the
  wire keeps what [`control-plane-api.md`](control-plane-api.md) §4 documents — a single string or `null`,
  which is what `server/src/http.rs` builds today — so no v0.x client is broken by this document. The switch
  belongs to the same batch as the prefix change.
- **[settled]** **`message` is for a person; `code` and `cause` are for a program.** A client that
  matches on `message` is broken by design.

## 3. `DispatchError`: the v1.0 shape

**[settled]** Six variants. `NoSuchAgent` is what v0.8 shipped and is **kept**; the other five arrive
with cross-device transport, which [`control-plane-api.md`](control-plane-api.md) §4 already
foreshadows ("cross-device transport will extend it (timeouts, remote errors)").

```rust
enum DispatchError {
    /// Kept from v0.8: no handle in this dispatcher owns `task.target`.
    NoSuchAgent(AgentId),

    /// The task reached a transport and the transport failed.
    Network { kind: NetworkKind },

    /// The executor was reached and declined — policy, capability, or a full queue.
    Refused { reason: String },

    /// The executor was reached and died. `None` means "died without a status".
    Crashed { exit_status: Option<i32> },

    /// The executor was reached and ran *some* of the task: `failed` items did not run.
    Partial { completed: usize, failed: usize },

    /// The request was malformed before it went anywhere.
    Invalid { reason: String },
}

enum NetworkKind { Timeout, Unreachable }
```

- **[settled]** **`Failed(String)` is replaced, not kept.** The catch-all that v0.8 shipped ("the
  executor was reached but the run failed") is what made retrying a coin toss; every dispatch failure now
  says which of the five it is. Nothing that sends `DispatchError` ships before v1.0, so no shim is
  needed — but the mapping in [`control-plane-api.md`](control-plane-api.md) §4 (which today reads
  `Failed → internal`) is replaced by §8's table in the same batch that lands this enum.
- **[settled]** **Variants are named, not numbered, on the wire.** The enum is tagged by variant name
  (`{"Network":{"kind":"Timeout"}}`), which is the shape v0.8's two variants already serialise to, so a
  reader that only knows two of the six fails loudly instead of misreading a number.
- **[settled]** **`Partial` is not a success.** It means some of the work did not happen; a caller that
  treats it as success reports work it did not get (see §4).

## 4. Which failures are retryable

**[settled]** Every variant answers this question, and it is the only reason the `Retryable` flag exists
(decisions §12). The flag is a property of the **error**, never of the caller's patience: a client may
choose to retry a refusal, but the model does not call it retryable.

| Variant | `retryable` | Why, and how |
|---|---|---|
| `Network::Timeout` | **true** | The attempt may not have reached the executor at all. Retry, ideally with jitter so a fleet does not synchronise. |
| `Network::Unreachable` | **true** | The peer is gone *now*; it is a transient condition by definition. Retry with **backoff**. |
| `Refused` | **false** | The executor was reached and said no: missing capability, a policy the deployer set, a full queue. Retrying an identical request asks the same question again and gets the same answer. |
| `Crashed` | **false** | Retrying does not revive a process. The task may be retried **after** the executor is restarted, and that restart is somebody's explicit act. |
| `Partial` | **partial** | Retry **only what failed** (the `failed` items). Re-running what `completed` claims is how work is duplicated. |
| `Invalid` | **false** | The caller's input is wrong. The same input fails identically; the fix is upstream. |
| `NoSuchAgent` | **false** | The target does not exist here, which maps to `not_found` (§8). Retrying cannot make it exist. |

- **[settled]** **A retry is identical.** The flag says whether *the same request* may succeed; a request
  that has to be modified to succeed is not a retry, it is a new request.
- **[settled]** **Backoff is the caller's.** The kernel states `retryable` and nothing about timing; a
  supervisor chooses its own schedule. This is red line 1 (mechanism, not policy) applied to retries.

## 5. The `Cause` chain

- **[settled]** **In process**: `Box<dyn std::error::Error>` where a real source exists, and a string
  where the source is a refusal with a sentence. A variant carries data, not prose, wherever the caller
  can branch on it (`exit_status`, `completed`, `failed`, `kind`).
- **[settled]** **On the wire**: the chain is flattened into `cause` (an array from the next prefix move, §2),
  outermost first, with each element a short, stable token or field name — the same vocabulary a client
  can switch on, never a sentence that changes between releases.
- **[settled]** **A chain never carries a secret.** The rule for `message` (§2) applies to every element:
  a path, a field name, a subsystem — never a token, a key or a URL with a credential in it.

## 6. Errors and the audit chain

- **[settled]** **A failed tool call is recorded, not hidden.** An executor's tool failure is written as
  `agent.tool.result` with `ok: false` and the reason, so the chain holds what the model was told
  ([`agent/src/audit_hook.rs`](../agent/src/audit_hook.rs), [`agent/src/tools.rs`](../agent/src/tools.rs)).
- **[settled]** **An error is information; a retry is an act.** A retry that succeeds still leaves the
  first failure in the chain. The chain is append-only: it records what happened, not what the system
  wishes had happened.
- **[settled]** **What is not audited**: nothing about a *dispatch* failure is written by the kernel
  itself today. When M (§9 of the roadmap) retries a task, the retry is M's act and appears as M's event;
  the kernel does not grow a retry log.

## 7. Errors and capabilities

- **[settled]** **`Refused` is often a capability answer.** The most common refusal is the one
  [`control-plane-api.md`](control-plane-api.md) §4 already defines: the actor lacks the endpoint's
  capability, which is `403 forbidden` — and 403 is **only ever** authentication or authorisation, never
  "a parameter I did not like" (that is a 400).
- **[settled]** **The refusal says which capability.** Where the information is not a secret — that is,
  always: capabilities are public — the `message` names the capability that was required and the
  `cause` names `capability`. A caller that cannot tell "I am not allowed" from "that was malformed" will
  fix the wrong thing.

## 8. The HTTP mapping

**[settled]** The rule: `403` is only ever authentication or authorisation; a parameter the server cannot
use is a `400`; a dependency that is not ready is a `503`. The table extends
[`control-plane-api.md`](control-plane-api.md) §4's list; the rows marked **v1.0** are new, and they land
with the prefix change so no v0.x client sees them.

| `code` | HTTP | When | Source |
|---|---|---|---|
| `bad_request` | 400 | Malformed JSON, a missing required field, a bad `path`/`run_id` | existing |
| `unauthorized` | 401 | No token, or the hook refused the token | existing |
| `forbidden` | 403 | Authenticated, but the actor lacks the capability | existing |
| `not_found` | 404 | Unknown `run_id`, `session_id`, snapshot name, or an unknown dispatch target | existing (`NoSuchAgent`) |
| `method_not_allowed` | 405 | The path is served, but not under this method | existing |
| `conflict` | 409 | A state clash: VM absent on `resume`, a download already running | existing |
| `payload_too_large` | 413 | A body over the endpoint's limit | existing |
| `internal` | 500 | Anything else — including a crashed executor | existing (`Crashed`) |
| `not_implemented` | 501 | A reserved endpoint with no kernel method yet | existing |
| `unavailable` | 503 | A dependency is not ready (no LLM, no QEMU, no toolchain) — and, **v1.0**, an unreachable peer | existing (`Network::Unreachable`) |
| `gateway_timeout` | 504 | **v1.0**: the peer was reachable in principle and did not answer in time | new (`Network::Timeout`) |

- **[settled]** **`Partial` is the one row still open** — see §9. Today it maps to `500 internal` with
  `cause: ["partial"]`, because the request genuinely did not complete; a dedicated status is not
  decided.
- **[settled]** **A transport failure of the kernel's own making is not a `500`.** `502` and `504` exist
  to say "the fault is between us"; using `500` for them tells a caller to look in the wrong place.

## 9. What is not settled

- **[open]** **`Partial`'s status**: `500` with a `partial` cause, or a `code` of its own. Both are
  defensible; the choice belongs to the batch that lands the enum.
- **[open]** **Whether any new `code` may appear in `/v0/`.** The list is closed for v0.9, and the honest
  reading is that the additions above arrive with the next prefix move — but a fix that needs one earlier is a
  judgement call, not a rule.
- **[open]** **The exact JSON of the `cause` chain** (an array of tokens versus an array of objects with
  a `kind`). The shape is decided when the first endpoint returns one.
