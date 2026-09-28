[中文](README.zh-CN.md) | English

# net

The **connection layer** (v1.0, [roadmap §4](../docs/roadmap-v1.0.md)), as
[`docs/connection.md`](../docs/connection.md) freezes it: a node's identity (§2), signing
(§3), discovery (§4), rooms (§5) and the cross-region server (§6).

**What is here today: §2, §3, §3.1, §4, §5 and §6 (except its digests).** A node's identity is an Ed25519 key pair in
`<data-dir>/node.key` — one JWK whose first member is `schema_version`, written owner-only,
minted on the first start that has networking configured and never minted by a read. A
message is `{v, from, to, ts, body}` signed over its canonical JSON, with `sig` beside it,
and `verify` runs the six checks [§3](../docs/connection.md) freezes, in that order, answering
with a category from the error model. Replay protection is per peer, in memory, over −5 min /
+1 min. The transport sends a frame as **one JSON line over a TCP socket** (`std`, no async
runtime), **direct first** and through the `Relay` seam when that fails. Discovery knows where
to dial: `peers.json` is authoritative for its own node, a handed-down `NodeTable` merges as a
**source** with conflicts reported, and a **UDP beacon** may only refresh an address — never
introduce a key. And `rooms.json` says who is in which room: membership, plus the three rules
(`rate` per member, `mention` defaulting to `nobody`, `require_signature`). And the cross-region
server is up as `RelayServer`, serving §6.2's roles: it **relays** a frame down the destination's
**session** — because §6.3 has the server wait to be dialled and **never dial a node**, which is
why this project needs no hole punching — it answers **signalling** with where a `node_id` can be
reached (**addresses, never payloads**), and it answers **management** with its registry: the node
table and the room definitions, a **source and not an authority**, merged with the node's own
`peers.json` and `rooms.json` winning. Authenticating is §3's model in every case — no new
credential, and no new capability. **Registration and heartbeat** (§6.6) are here too: a node reports
itself upward, the server keeps an [`OnlineTable`], and a row that goes `offline` is kept. §6.7's
liveness judgement is not (V-3).
**Not here yet: §7 — the audit digests** a server aggregates on a timer, which wait on M5's
authorisation. §6.7's **liveness judgement** — the probes, the reports and the collective threshold —
is **V-3**'s. Each piece arrives only after the section it implements is frozen, which is
what [decisions §3](../docs/decisions.md) asks for and what keeps the cross-device work from having
to be done twice.

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
| `src/rooms.rs` | `rooms.json`: membership (a member is a **`node_id`**), the three rules (`RateRule` and `RateCounters`, `Mention`, `require_signature`), and the checks a room must pass to load. |
| `src/relay.rs` | The cross-region server's relay: `RelayServer` (parse, authenticate, route on `to`, hand the frame down the destination's session), `SessionTable` (who is dialled in — which is what the server never dials around), `RelayClient` / `RelaySession` (the node's half, and the `hello` that opens a session), and `Forwarder`, the one-method seam that keeps the routing rule testable with no socket near it. `src/bin/riscdom-relay.rs` is the program a **deployer** runs. |
| `src/registry.rs` | The **management** plane: `Registry` — §4.1's hand-down table with the room definitions beside it, carried as one signed frame — and `merge`, which applies §4.1 to **both** halves so the local `peers.json` and `rooms.json` win and every disagreement is **reported**. A published room set is held to `rooms.json`'s own checks, so a source cannot carry a room a file would refuse. |

## Running it

```text
cargo run -p net --example identity -- --self-test
cargo run -p net --example sign -- --self-test
cargo run -p net --example transport -- --self-test
cargo run -p net --example discovery -- --self-test
cargo run -p net --example rooms -- --self-test
cargo run -p net --example relay -- --self-test
```

All six are the shape the gate uses for the other example-level proofs
(`worker/examples/remote_executor.rs`): they work in a scratch directory, in memory, or on
loopback, and check what the protocol freezes — the key file's shape and permissions; all six
verification refusals plus the rule that a key rotation does not reset a peer's replay mark;
that a message round-trips, that both transport paths carry **identical** bytes and that a
failure maps onto the error model's category; that a peer file reads back, that a conflict is
reported, that a beacon verifies over UDP and that a filter denies by default; for `rooms`,
that the file loads, that a room may not drop the signature floor, that the budget is per member
and that the filter reads membership out of the file; and, for `relay`, that a frame reaches a
dialled-in destination **byte for byte**, that a frame addressed to the server itself is never
handed on, that an unknown sender or destination is refused, that a replay arrives once, that
the server **never dials** a destination that has not dialled in, that an address query is
answered with the addresses the server knows **and nothing else**, and that a published registry
is a **source** — merged, and merged with the local files winning, that a node **registers** and
**beats** into the server's online table (§6.6), and that `register` and `registry` are read as two
different frames. `scripts/gate.sh` runs all six;
the deployer's program is `cargo run -p net --bin riscdom-relay -- --help`.

## What this crate does not do yet

The **audit digests** of §7 — what a server aggregates on a **30-second** timer, and how a key
event is pushed — which wait on M5's authorisation. §6.2's other three roles are here.
[`NoRelay`](src/transport.rs) remains what a deployment with no cross-region
server configured wires: §6.1 says such a deployment loses only the wide-area lane. The keyring route for
`node.key` — §2 allows the key to live in the OS keyring — is also not wired: the file path
landed first, and `NodeKey` is a value a caller can hand to whichever store it prefers. **The
node's peer port is configuration** (§4.3) and belongs to the host's settings, which this crate
does not touch. **Authorisation is not here either, on purpose**: this crate answers *who sent
this*, *what the room permits* and *who may be adopted*, and whether a node may do the thing is
the capability model's question ([security-model.md §4](../docs/security-model.md)).

[`audit::canonical_json`]: ../audit/src/run.rs
[`audit::fingerprint`]: ../audit/src/run.rs
