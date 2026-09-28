[中文](README.zh-CN.md) | English

# net

The **connection layer** (v1.0, [roadmap §4](../docs/roadmap-v1.0.md)), as
[`docs/connection.md`](../docs/connection.md) freezes it: a node's identity (§2), signing
(§3), discovery (§4), rooms (§5) and the cross-region server (§6).

**What is here today: §2, §3, §3.1 and §4.** A node's identity is an Ed25519 key pair in
`<data-dir>/node.key` — one JWK whose first member is `schema_version`, written owner-only,
minted on the first start that has networking configured and never minted by a read. A
message is `{v, from, to, ts, body}` signed over its canonical JSON, with `sig` beside it,
and `verify` runs the six checks [§3](../docs/connection.md) freezes, in that order, answering
with a category from the error model. Replay protection is per peer, in memory, over −5 min /
+1 min. The transport sends a frame as **one JSON line over a TCP socket** (`std`, no async
runtime), **direct first** and through the `Relay` seam when that fails. And discovery knows
where to dial: `peers.json` is authoritative for its own node, a handed-down `NodeTable`
merges as a **source** with conflicts reported, and a **UDP beacon** may only refresh an
address — never introduce a key.
**Not here yet: rooms' own file (§5) and the cross-region server (§6)** — including the relay's
routing, which is why `NoRelay` exists. Each piece arrives only after the section it implements
is frozen, which is what [decisions §3](../docs/decisions.md) asks for and what keeps the
cross-device work from having to be done twice.

## Dependency direction

`net` depends on [`audit`](../audit/README.md) and on nothing else in this workspace. The
chain's canonical JSON and hashing ([`audit::canonical_json`], [`audit::fingerprint`]) are
what a signature is computed over and what a key fingerprint is taken of, so there is one
description of those bytes instead of two. `host-core` is what will depend on *this* crate
— never the other way round: the direction is `audit ← net ← host-core ← server`.

## The files that matter

| File | What it is |
|---|---|
| `src/versioned.rs` | One implementation of "a JSON file whose first member is `schema_version`": four outcomes for a read (`Missing` / `Current` / `Migrated` / `TooNew`), a newer file **refused** rather than half-read, and an owner-only write for secrets. `peers.json` and `rooms.json` (M4b, M4c) are meant to reuse it. |
| `src/identity.rs` | `NodeKey`: mint, save, load, and the checks that make a file a *usable* JWK (right `kty`/`crv`, both halves 32 bytes of base64url, and `x` re-derived from `d` and compared — a spliced file is refused). |
| `src/message.rs` | `SignedMessage`: the five-member preamble, the canonical bytes it is signed over (`audit::canonical_json`), and the one-line wire form. |
| `src/sign.rs` | The six verification steps in order, `PeerKeys` (a **set** of keys per peer, so a rotation's grey period works), and `VerifyError`'s mapping onto the error model's categories. |
| `src/replay.rs` | `ReplayGuard`: per peer, in memory, a high-water mark plus the payloads seen at it; advancing the mark discards the set. |
| `src/transport.rs` | `Connection` and `Listener`: one JSON line per message over a **std** TCP socket, the frame serialised once, sent direct first and through the `Relay` seam second. Ports and timeouts live in `TransportConfig`, because §3.1 does not freeze them. |
| `src/error.rs` | The error model's five categories, in one place — the verifier and the transport both map their refusals onto this. |
| `src/peers.rs` | `peers.json`: the entry shape a handed-down table shares, the local file that wins, and the rule that a peer table holds **public** keys only. |
| `src/discovery.rs` | `NodeTable` (the hand-down, with its generation, merged as a **source** and reporting conflicts), the UDP beacon (`sign_announcement` / `receive_datagram`) and `RoomFilter` — the default-deny test that keeps a beacon from introducing a key. |

## Running it

```text
cargo run -p net --example identity -- --self-test
cargo run -p net --example sign -- --self-test
cargo run -p net --example transport -- --self-test
cargo run -p net --example discovery -- --self-test
```

All four are the shape the gate uses for the other example-level proofs
(`worker/examples/remote_executor.rs`): they work in a scratch directory, in memory, or on
loopback, and check what the protocol freezes — the key file's shape, permissions, the
`TooNew` refusal and "a read never mints"; all six verification refusals plus the rule that a
key rotation does not reset a peer's replay mark; that a message round-trips, that one
connection carries several frames, that both paths carry **identical** bytes and that a
failure maps onto the error model's category; and, for `discovery`, a peer file that reads
back, a conflict that is reported, a beacon that verifies over UDP, and a filter that denies
by default. `scripts/gate.sh` runs all four.

## What this crate does not do yet

Rooms' own file (§5 — the filter here reads a room set, and `rooms.json`'s shape is M4c's) and
the cross-region server (§6) — including the relay's routing, which is why
[`NoRelay`](src/transport.rs) is the only `Relay` implementation here. The keyring route for
`node.key` — §2 allows the key to live in the OS keyring — is also not wired: the file path
landed first, and `NodeKey` is a value a caller can hand to whichever store it prefers. **The
node's peer port is configuration** (§4.3) and belongs to the host's settings, which this crate
does not touch. **Authorisation is not here either, on purpose**: this crate answers *who sent
this*, and whether that node may do the thing is the capability model's question
([security-model.md §4](../docs/security-model.md)).

[`audit::canonical_json`]: ../audit/src/run.rs
[`audit::fingerprint`]: ../audit/src/run.rs
