[中文](README.zh-CN.md) | English

# net

The **connection layer** (v1.0, [roadmap §4](../docs/roadmap-v1.0.md)), as
[`docs/connection.md`](../docs/connection.md) freezes it: a node's identity (§2), signing
(§3), discovery (§4), rooms (§5) and the cross-region server (§6).

**What is here today: the crate's skeleton, and §2.** A node's identity is an Ed25519 key
pair in `<data-dir>/node.key` — one JWK whose first member is `schema_version`, written
owner-only, minted on the first start that has networking configured and never minted by a
read. Nothing else has landed yet: **no network code, no transport, no signing.** Each
piece arrives only after the section it implements is frozen, which is what
[decisions §3](../docs/decisions.md) asks for and what keeps the cross-device work from
having to be done twice.

## Dependency direction

`net` depends on [`audit`](../audit/README.md) and on nothing else in this workspace. The
chain's canonical JSON and hashing ([`audit::canonical_json`], [`audit::fingerprint`]) are
what a signature is computed over and what a key fingerprint is taken of, so there is one
description of those bytes instead of two. `host-core` is what will depend on *this* crate
— never the other way round: the direction is `audit ← net ← host-core ← server`.

## The two files that matter

| File | What it is |
|---|---|
| `src/versioned.rs` | One implementation of "a JSON file whose first member is `schema_version`": four outcomes for a read (`Missing` / `Current` / `Migrated` / `TooNew`), a newer file **refused** rather than half-read, and an owner-only write for secrets. `peers.json` and `rooms.json` (M4b, M4c) are meant to reuse it. |
| `src/identity.rs` | `NodeKey`: mint, save, load, and the checks that make a file a *usable* JWK (right `kty`/`crv`, both halves 32 bytes of base64url, and `x` re-derived from `d` and compared — a spliced file is refused). |

## Running it

```text
cargo run -p net --example identity -- --self-test
```

The self-test is the shape the gate uses for the other example-level proofs
(`worker/examples/remote_executor.rs`): it works in a scratch directory under the system
temp, mints a key, reads it back, and checks the file's shape, its permissions, the
`TooNew` refusal and the "a read never mints" rule. `scripts/gate.sh` runs it beside the
others.

## What this crate does not do yet

Signing (§3), the transport (§3.1), replay protection (§3.2), discovery (§4), rooms (§5)
and the cross-region server (§6). The keyring route for `node.key` — §2 allows the key to
live in the OS keyring — is also not wired: the file path landed first, and `NodeKey` is a
value a caller can hand to whichever store it prefers.

[`audit::canonical_json`]: ../audit/src/run.rs
[`audit::fingerprint`]: ../audit/src/run.rs
