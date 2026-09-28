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
- **[open]** **Not frozen even inside §2–§3**: the transport a signed message travels over (M4d), the
  replay window's size and where the seen-set lives ([§3](#3-signing-)), and the wire form of the
  room a message is addressed to (M4c).

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
signature-beside-token rule; and "authentication here, authorisation there". **Not frozen**: the
transport, the replay window's size and the store that remembers seen messages, the room form of
`to` (M4c), key rotation and revocation in the wire protocol ([decisions §13](decisions.md) fixes
their *shape* — parallel rotation, a broadcast revocation list — and M4d is where they travel), and
whether a message may be signed by more than one key.

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
leaving the data path ([roadmap §4](roadmap-v1.0.md)). This is where a signed message's **transport**
is decided, and therefore where §3's open items (the transport, the replay store) close.

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
