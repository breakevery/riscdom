[中文](connection.zh-CN.md) | English

# The connection layer

**Status** v1.0 specification (milestone [M4](roadmap-v1.0.md)) ｜ **Date** 2026-09-28 ｜ **Baseline**
v0.9.9 (`3365970`) ｜ **Audience** kernel developers and deployers — whoever runs more than one node.

**What this document is.** Layer two's rules ([roadmap §4](roadmap-v1.0.md)) written down so they can
be **frozen before they are built**. M4 is the first milestone this project builds from nothing: the
reconnaissance found no network code at all, no key material, and no parsing for `node.key`,
`peers.json` or `rooms.json` — only the decisions behind them
([§7](decisions.md), [§13](decisions.md), [§33](decisions.md)) and the seams that will host them
(the `Authn` hook, `NetworkSettings`, `audit`'s digests, the keyring wrapper, `agent::identity`'s
device-partitioned ids).

**It is written in parts.** M4 is split into five pieces, and this document grows with them:
**M4a identity and signing** (written below), M4b discovery, M4c rooms, M4d the cross-region server,
M4e audit digests. A section that is not written yet says **deferred** and names the piece it
belongs to; nothing deferred is a promise about its shape.

**Its companions.** [decisions §13](decisions.md) (credentials and key management) and
[§7](decisions.md) (cross-device) are the decisions behind §2 and §3;
[security-model.md](security-model.md) §1 says where each secret lives;
[api-compatibility.md](api-compatibility.md) §6 is where the new formats are registered;
[error-model.md](error-model.md) is the vocabulary a failure travels in.

Each written section ends with what it **freezes** and what it **leaves open**.

## 1. What is frozen, and what is not

- **[settled]** **Frozen here**: node identity (the key pair, its file, its generation) in §2, and
  signing (`@`, the signed bytes, verification, how it sits beside the bearer token) in §3, plus the
  two standing constraints in §8 and the trust model in §9 and the red-line test in §10.
- **[open]** **Deferred**: discovery (M4b), rooms (M4c), the cross-region server (M4d) and audit
  digests (M4e — which also waits on M5's authorisation). §4–§7 are titles, not shapes.
- **[open]** **Not frozen even inside §2–§3**: the **port numbers** a node listens on, the
  connect/read/write **timeouts**, the replay record's in-memory shape and whether a later batch
  persists it (§3.1, §3.2 — the *shape* of the transport and of the window **is** frozen), and the
  wire form of the room a message is addressed to (M4c).

**Frozen**: identity and signing, as stated in §2 and §3. **Not frozen**: everything §4–§7 name and
everything marked *open* above. Silence is not a promise.

## 2. Node identity

- **[settled]** **A node's cryptographic identity is an Ed25519 key pair** ([decisions §13](decisions.md)): one private key it signs with, one public key its peers verify with. Nothing home-grown — the curve and the encoding are standards, and the project adds one field to the container rather than a format of its own.
- **[settled]** **Where it lives**: `<data-dir>/node.key` by default, **the OS keyring optionally** ([decisions §13](decisions.md), [security-model.md](security-model.md) §2). The file's mode is **`600`** (on Windows, an owner-only ACL); a file whose permissions cannot be restricted is refused rather than used — the same rule the bearer token already follows ([security-model.md](security-model.md) §2).
- **[settled]** **The file is one JWK** ([RFC 7517](https://www.rfc-editor.org/rfc/rfc7517) /
  [8037](https://www.rfc-editor.org/rfc/rfc8037) `OKP`/`Ed25519`), JSON, UTF-8, **no BOM**, with one
  member the JWK spec does not define and which we require first:

  ```json
  {
    "schema_version": 1,
    "kty": "OKP",
    "crv": "Ed25519",
    "x": "<base64url public key, 32 bytes>",
    "d": "<base64url private key, 32 bytes>"
  }
  ```

  **Why a JWK and not a PEM**: [decisions §13](decisions.md) allows either, and
  [decisions §11](decisions.md) requires every persisted format to carry `schema_version` **as its
  first field**. A PEM's first field is its `BEGIN` line, so a versioned PEM would need a container —
  a second format. A JWK is standard, is already JSON like every other file this project writes, and
  the spec lets a member be added: `schema_version` is that member, and a reader that does not know
  it must ignore it.
- **[settled]** **First field means first**: `schema_version` is the first member written and the first one read. A file with a **newer** `schema_version` is refused (`data_too_new`, nothing read, nothing written); an older one is migrated on open, per [api-compatibility.md](api-compatibility.md) §6.
- **[settled]** **Generated on first start, never on a read.** A node that has no key mints one the first time it starts with networking configured — the rule the bearer token already follows ([`server/src/token.rs`](../server/src/token.rs), [security-model.md](security-model.md) §2: "reading a credential never creates one"). A node with no networking configured mints nothing.
- **[settled]** **The key is not the node's name, and not the process id.** `agent::identity` mints an `<device>-<pid>-<seq>` `AgentId` for every process, and the **device** part is the node's name (default `local`; the connection layer is what sets it, [roadmap §4](roadmap-v1.0.md)). The key pair is a **separate, longer-lived** identity: it survives every restart, every pid, and every rename. What a peer stores in `peers.json`'s `node_id` is the **device name**, and what it verifies with is that name's public key.

**Frozen**: Ed25519; the file, its path and its mode; the JWK shape and the `schema_version` member
first; generation on first start with networking; and the separation between the key pair, the device
name and the process `AgentId`. **Not frozen**: whether `node.key` may live in the keyring in every
deployment ([security-model.md](security-model.md) §8 already lists this as open), and how a device
name is chosen or changed.

## 3. Signing `@`

- **[settled]** **`@` means address *and* signature** ([decisions §7](decisions.md)): a message addressed with `@` is signed by the key its sender names. This section fixes **what is signed**, not how the message travels.
- **[settled]** **What is signed is the canonical JSON of a five-member preamble** — `{"v", "from", "to", "ts", "body"}` — serialised with the same canonical discipline `audit`'s chain uses ([`audit::canonical_json`](../audit/src/lib.rs): object keys in a fixed order, no insignificant whitespace). The signature is Ed25519 over exactly those bytes.

  | Member | What it is | Why it is inside the signature |
  |---|---|---|
  | `v` | the protocol major | a signature cannot be replayed into another protocol version |
  | `from` | the sender's `node_id` (its device name) | a valid signature cannot be re-attributed |
  | `to` | the recipient's `node_id`, or the room when one is addressed | a signed message cannot be redirected to another node |
  | `ts` | epoch milliseconds, the sender's clock | the receiver's window can be enforced |
  | `body` | the payload | the payload cannot be altered |

- **[settled]** **The signature travels beside the message**, in a member named `sig`, and the preamble travels with it (a receiver needs `from` and `ts` to verify, and needs `body` byte-identical to what was signed).
- **[settled]** **Verification, in this order, and it stops at the first failure:**
  1. the receiver **knows the sender** — `from` is in its own `peers.json` (or the message's room, M4c);
  2. **the signature verifies** against that entry's public key, over the canonical bytes above;
  3. **`v` is a protocol version the receiver speaks** — a newer major is refused, not guessed at ([api-compatibility.md](api-compatibility.md) §6);
  4. **`to` is this node** (or a room it is in);
  5. **`ts` is inside the receiver's window** — outside it, the message is refused rather than queued;
  6. **the message is not a replay** — a `(from, ts, body-hash)` already seen inside that window is refused.
  A failure at any step is an answer, never a silent drop: the reason maps onto [error-model.md](error-model.md)'s categories (`refused` for an unknown sender or a replay, `invalid` for a broken signature or a version mismatch, `network` for a stale-or-future timestamp).
- **[settled]** **Signing is a layer *above* the bearer token, not a replacement.** The bearer token is the **local** control plane's door — one machine, one secret on disk ([security-model.md](security-model.md) §3) — and it is never sent to a peer: two nodes share no secret, only public keys. A node-to-node message therefore carries a **signature** where a local call carries a **token**, and the two are checked by different code on purpose. A node that also runs a control plane still answers local calls with the token; the signature adds nothing there and changes nothing there.
- **[settled]** **Signature is authentication; capability is authorisation.** Verifying a signature says *who* sent the message. Whether that node may do the thing is the existing capability question ([security-model.md](security-model.md) §4), answered by the same model as everywhere else — a correctly signed message from a node whose declared capabilities do not include the act is **refused**, exactly as a valid token with too few capabilities is refused today.

**Frozen**: the `@` meaning; the five-member preamble and the canonical encoding it is signed over;
the `sig` member; the six verification steps and their order; the error mapping; the
signature-beside-token rule; and "authentication here, authorisation there". Its two open items are
closed below — the transport in **§3.1**, replay protection in **§3.2**. **Not frozen**: the port
numbers and the timeouts (§3.1), the replay record's data structure and whether it is ever persisted
(§3.2), the room form of `to` (M4c), key rotation and revocation in the wire protocol
([decisions §13](decisions.md) fixes their *shape* — parallel rotation, a broadcast revocation list —
and M4d is where they travel), and whether a message may be signed by more than one key.

### 3.1 The transport

- **[settled]** **A signed message moves as one JSON line over a TCP connection** — the discipline `worker` and [plugin-interface.md](plugin-interface.md) already use for a stream: one JSON object per line, UTF-8, `\n`-terminated, no embedded newlines. TCP is what [decisions §7](decisions.md) names for a dispatch to a peer, and a line-delimited frame is what the rest of the project already parses without a second grammar.
- **[settled]** **Direct first, relay second.** A sender tries the peer's own addresses first (the `addresses[]` its `peers.json` entry carries, or whatever M4b discovered). If that fails — connect refused, unreachable, or a timeout — the message is sent to the **relay**, which is the **main path**, not the exception ([roadmap §4](roadmap-v1.0.md), [decisions §7](decisions.md)). RiscDom builds **no hole punching**: the relay is a bridge both sides dial out to.
- **[settled]** **The frame is the same whether direct or relayed** — byte for byte. That is what makes the relay **stateless**: it forwards a line it cannot usefully alter, because the signature covers `from`, `to`, `v` and `ts` (§3). A relay that rewrote a frame would break the signature, so the relay has no authority over the message.
- **[settled]** **The connection is plaintext TCP, and its integrity comes from the signature rather than from the transport.** This project ships no TLS on this path ([security-model.md](security-model.md) §3): confidentiality is the deployer's (a private network or a TLS terminator), while *who* and *unaltered* are the signature's job. A peer is never sent the bearer token — two peers share no secret ([§3](#3-signing-)).
- **[settled]** **No separate transport handshake.** The first frame on a connection is a message; a connection that does not deliver a complete, well-formed frame inside the read timeout is closed. The protocol version is checked **per message** (`v`, §3 step 3) rather than negotiated once, because a connection carries messages and each one stands on its own.
- **[settled]** **Where the bytes go, and where they do not.** This section fixes the frame and how it moves between **two nodes that can reach each other** — directly, or by handing it to a relay it can reach. **M4d** ([§6](#6-the-cross-region-server--deferred-m4d)) decides the relay's own rules: how it routes a frame it has received, who may ask it to, and how a node learns where it is. The two do not overlap — nothing here says what a relay does with a line, and nothing there changes the line.

**Transport failures map onto [error-model.md](error-model.md)'s categories, and `partial` is not used here** — it describes a batch, and one frame is one message:

| What happened | Category | Retryable |
|---|---|---|
| cannot connect, or a read/write times out | `network` | yes ([error-model.md](error-model.md) §4) |
| the connection ends before a complete frame arrives | `network` | yes |
| a complete frame does not parse, its `v` is newer, its signature is bad, or `to` is not us | `invalid` | no |
| the sender is unknown, or the message is a replay | `refused` | no |
| the peer answers with an explicit refusal | `refused` | no |

**Frozen**: one JSON line per message over TCP; direct-first-then-relay; the frame being identical on both paths, and therefore the relay's statelessness; plaintext-with-signature and no token between peers; no separate handshake, with `v` checked per message; the boundary with M4d; and the error mapping above. **Not frozen**: the **port numbers** a node listens on or a relay uses (a node's addresses are discovery's business, M4b), the connect/read/write **timeouts**, whether one connection carries several messages or one, and any transport-level compression or batching.

### 3.2 Replay protection

- **[settled]** **The window is five minutes behind and one minute ahead** — a `ts` older than *now − 5 min*, or more than *now + 1 min* in the future, is **refused** rather than queued (§3 step 5). The backward side is sized by the design's own longest documented delay: [decisions §33](decisions.md) allows a **30 s – 2 min** silent-retry period before anything escalates, and a window a legitimate retry could fall out of would refuse the retries this project depends on. The forward side exists because clocks differ — a minute is enough for skew and too little to hide a forged future.
- **[settled]** **The record is per peer, in memory, and it is a high-water mark plus a small set.** For each peer the receiver keeps the **highest `ts` it has accepted** from that peer, and the set of **body hashes accepted at that same `ts`** — a message and its answer, or two messages minted in the same millisecond, share a `ts` and are both legitimate. A message is a replay when its `ts` is **older than** the high-water mark, or **equal to** it and its `(from, ts, body-hash)` is already in the set. A **newer** `ts` is accepted, and the record advances.
- **[settled]** **Cleanup needs no timer.** Advancing the high-water mark **discards the set**: those hashes sit at a `ts` the window will refuse anyway. Nothing accumulates and nothing is swept — the record's size is bounded by "one timestamp's worth of messages from one peer".
- **[settled]** **The record is not persisted, and that is stated rather than hidden.** A restart forgets every high-water mark, so a message still inside the five-minute window can be replayed **once** across a restart. The exposure is bounded by the window, and it shrinks as traffic advances the record again. Whether a later batch persists it — which would make it a **new on-disk format**, registered in [api-compatibility.md](api-compatibility.md) §6 like the others — is left open below.
- **[settled]** **The record belongs to the peer, not to the key.** Key rotation runs several keys in parallel ([decisions §13](decisions.md)), so step 2 verifies against **all** the public keys the peer's entry currently carries — and a rotation must **not** reset the record, or a replayed message would be accepted again the moment a new key appeared. **Revocation** removes a key from the set, after which messages under it are `refused` as from an unknown sender; delivering a revocation is M4d's, and M4a carries none.

**Frozen**: the window (−5 min / +1 min) and the argument that sizes it; the per-peer, in-memory
high-water-mark-plus-set record; the advance-and-discard cleanup; that the record is per peer and
survives a rotation; and the honest limit that a restart loses it. **Not frozen**: the record's
in-memory data structure, whether a later batch persists it (a new format, to be registered), the
skew allowance a particular deployment may want, and anything about how a revocation is delivered
(M4d).

## 4. Discovery — deferred (M4b)

**Deferred to M4b.** The static node table the in-network server hands down, and UDP broadcast with
room isolation as the supplement ([roadmap §4](roadmap-v1.0.md), [decisions §7](decisions.md)). Its
shape depends on §2 (what a node table entry is) and is written before it is built.

## 5. Rooms — deferred (M4c)

**Deferred to M4c.** `rooms.json`, membership and the rules a room carries — rate, who may `@` whom,
whether a signature is required ([roadmap §4](roadmap-v1.0.md), [decisions §7](decisions.md)). §3
already fixes what a *signature* is; M4c fixes what a room *demands of* one.

## 6. The cross-region server — deferred (M4d)

**Deferred to M4d.** A dedicated server with four roles — signalling, relay, management and audit
aggregation — the relay as the **main path** rather than the exception, and a direct connection
leaving the data path ([roadmap §4](roadmap-v1.0.md)). §3.1 already fixes what a frame **is** and
that it is byte-identical on both paths; M4d decides what the relay **does with** a frame it has
received — how it routes it, who may ask it to, and how a node learns where it is — so the two
sections do not overlap.

## 7. Audit digests — deferred (M4e, and authorised separately)

**Deferred to M4e.** Collecting the chain's digests on a **30-second** timer, with a key event pushed
the moment it happens ([roadmap §4](roadmap-v1.0.md)). It is written **last**, and it does not start
before the authorisation [decisions §33](decisions.md) requires for anything that touches the audit
boundary. The temporary centre, `provisional` and `fork` are **not M4's** — they are M5/M6.

## 8. Architecture independence

- **[settled]** **The identity layer assumes nothing about a guest.** §2 and §3 name no machine, no instruction set and no emulator: a node's key signs *messages*, not binaries, and a message body is opaque to the layer that carries it. Nothing here may grow a field that only makes sense for one architecture ([roadmap §4](roadmap-v1.0.md), [decisions §2](decisions.md)).
- **[settled]** **This does not conflict with the plugin interface.** [decisions §3](decisions.md) puts the *sandbox* out of process and [plugin-interface.md](plugin-interface.md) freezes that seam; this document is about *nodes*. The two meet nowhere: a plugin process has no key, signs nothing, and is never a peer.

**Frozen**: the constraint and the separation from the plugin seam. **Not frozen**: anything an
implementation of either layer adds internally.

## 9. Trust model

- **[settled]** **Identity is a signature layer, not an authorisation layer.** Verifying a signature establishes *who*; it grants nothing ([§3](#3-signing-)). Every authority decision stays with the capability model ([security-model.md](security-model.md) §4), so a node cannot gain power by holding a key — only by being granted capabilities.
- **[settled]** **The private key never leaves the node.** It is read to sign; it is never sent, never logged, never put in a message, and never in an argument ([security-model.md](security-model.md) §2). `peers.json` holds **public** keys only.
- **[settled]** **A peer is untrusted until it is known.** Nothing in §3 accepts a message from a node the receiver has not been configured to know: there is no "trust on first use" and no reputation by address. An unknown `from` is `refused`.
- **[settled]** **What a node does on a peer's behalf is on the chain.** An act the node performs because a signed message asked for it is an audit row like any other ([security-model.md](security-model.md) §7) — this layer authenticates the request and never becomes a second log.

**Frozen**: signature-is-not-authorisation, the private key's containment, no trust-on-first-use, and
"the chain is still the record". **Not frozen**: the deployment's own perimeter (which nodes may
reach which — [decisions §7](decisions.md) leaves topology to the deployer).

## 10. The red lines

[roadmap §1](roadmap-v1.0.md) states four constraints and calls them the test each milestone's work
has to survive. The constraints are cited, not repeated.

- **Not a general-purpose sandbox.** §8 states it for this layer: identity and signing name no architecture, and a message body is opaque.
- **No built-in supervisor.** Signing decides **who**, never **what**: §3 stops at the capability layer, and no rule here lets a node act on another's behalf by virtue of its key.
- **No officially operated service.** A key pair is a file on the deployer's machine and a signature is verified by the receiving node; this document names no party that serves anybody ([§6](#6-the-cross-region-server--deferred-m4d) is where that obligation is met, when it is written).
- **The audit invariants do not move.** §9 keeps the chain as the record, and nothing in §2 or §3 defines an audit event, a hash input or a chain operation — the digests of §7 are *transport* over the chain that already exists.

**Frozen**: no amendment to this document may fail one of the four. **Not frozen**: the four are the
test, not a licence to change what they mean.
