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
belongs to; nothing deferred is a promise about its shape. The **V batches** — the connection layer's
wiring into the host — then added **§6.6**, the reporting half of a server's node list
(registration and heartbeat): the one part of this document written **after** M4 was closed.

**Its companions.** [decisions §13](decisions.md) (credentials and key management) and
[§7](decisions.md) (cross-device) are the decisions behind §2 and §3;
[security-model.md](security-model.md) §1 says where each secret lives;
[api-compatibility.md](api-compatibility.md) §6 is where the new formats are registered;
[error-model.md](error-model.md) is the vocabulary a failure travels in.

Each written section ends with what it **freezes** and what it **leaves open**.

## 1. What is frozen, and what is not

- **[settled]** **Frozen here**: node identity in §2, signing (including its transport and replay protection in §3.1–§3.2) in §3, **discovery** in §4, **rooms** in §5 and **the cross-region server** in §6, plus the two standing constraints in §8, the trust model in §9 and the red-line test in §10.
- **[settled]** **Audit digests (M4e)**: the role is §6.2's, the shape is §7 below, and the authorisation
  it waited on was granted — [decisions §127](decisions.md). The last piece of M4 to be written, and now
  written.
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

## 4. Discovery

**Two sources, in this order of authority**: the **in-network server** hands a node the table of who is
on this network ([roadmap §4](roadmap-v1.0.md), agreement B — the network is not a flat peer-only
one), and a **UDP broadcast** lets a node that appeared between two hand-downs announce itself
([decisions §7](decisions.md) names UDP broadcast with room isolation as the supplement). §3.1 already
said how a frame **moves**; this section says **where an address comes from**.

### 4.1 The table the in-network server hands down

- **[settled]** **The in-network server is a node with a role**, not a new kind of process — the same kernel, differentiated by deployment ([decisions §33](decisions.md)). This section uses one part of that role: it holds the network's node table and hands it down. The rest (§33's centre, the cross-region server of §6) is not this section's.
- **[settled]** **An entry is exactly a `peers.json` entry** — `{node_id, addresses[], public_key, capabilities, rooms[]}`, the shape [decisions §13](decisions.md) fixed and [§2](#2-node-identity) already reads. One shape, one grammar: a table is a **list of entries**, so nothing has to translate between "a peer I was configured with" and "a peer I was told about".
- **[settled]** **The table is a source, not an authority.** A node merges what it is handed into its own view; **its own `peers.json` stays authoritative for itself**, and a conflict — the same `node_id` with a different key or different addresses — is **reported, never silently resolved**, the rule [plugin-interface.md](plugin-interface.md) §6 already states for manifest sources. A node keeps working when the table is stale, wrong or absent.
- **[settled]** **Where the server's table comes from**: its own `peers.json` (what a deployer wrote) plus the entries it has itself accepted from announcements (§4.2). What it chooses to publish is the deployer's policy, not this document's.
- **[settled]** **When it is handed down**: a node asks **at startup** (once its identity exists and networking is configured) and **on every reconnect** to the server, and the server **pushes** a new table when its own changes. Each table carries a **generation** — a monotone integer — so a node can tell whether the copy it holds is current. There is no polling loop and no fixed refresh interval.
- **[settled]** **The hand-down is a message, not a new protocol.** It travels as a signed frame over the transport §3.1 froze — the node connects to the server the way it connects to any peer — and its `body` carries the generation and the entries. §3's verification applies unchanged, and the server is simply a peer whose key the node knows.

### 4.2 UDP broadcast, the supplement

- **[settled]** **One datagram carries one signed frame, and its only permitted effect is to offer an address.** An announcement's `body` is the sender's **own entry** (the §4.1 shape) plus the rooms it announces for. It is a **beacon**: it expects no answer, it is best-effort, and a receiver may use it only to consider a candidate entry. The parts of §3 that matter — the signature, the window, the replay record — apply to it exactly as they do to any frame.
- **[settled]** **It is not the peer transport.** §3.1 governs the path between **two nodes that can reach each other**; a broadcast is a different, one-way carrier, and it is the **only** thing in this protocol that travels over UDP — no request, no answer and no hand-down uses it.
- **[settled]** **An announcement refreshes an address; it cannot introduce a key.** A node **already known** (in its own `peers.json` or the handed-down table) may have its `addresses[]` refreshed by an announcement; a node the receiver does **not** know is **reported** for the deployer to add, never adopted on the announcement's own authority. That keeps §9's rule that a peer is untrusted until it is known, without a **trust on first use** appearing sideways through discovery. The **in-network server** is the one that may adopt an unknown announcement into the table it hands down, under the deployer's policy — that is what its role is for.
- **[settled]** **Room isolation is a filter with a safe default.** An announcement names the rooms it is for; a receiver adopts one **only when the rooms it names intersect the rooms the receiver is configured for**, and a node with **no rooms configured adopts none** — the default-deny the rest of the project uses ([security-model.md](security-model.md) §4). The membership list itself is `rooms.json`, whose **shape is M4c's** ([§5](#5-rooms--deferred-m4c)): this section fixes the **filter**, not the file.
- **[settled]** **Isolation is a filter, not a wall, and the signature is what makes that enough.** A broadcast reaches every machine on the link, so anyone may **read** a datagram; what they cannot do is **use** one that was not meant for them. The payload is **signed** (§3), so a node cannot announce itself as another, and cannot announce an address whose key it does not hold. Payload **confidentiality is therefore not claimed** — a deployment that needs it does not enable broadcast on that segment.
- **[settled]** **The cross-region server is not here.** Discovery hands out **local** addresses, on the network a node is on; whether a node has a cross-region server, and where it is, comes from that node's own configuration ([§6](#6-the-cross-region-server--deferred-m4d)). Discovery neither learns about it nor hands one down.

### 4.3 Ports

- **[settled]** **A node's peer port is configuration, and every announced address carries the one that is true.** The port is a field of the node's own **network settings** — the settings section the in-network work already introduced — and what a peer records is the `addresses[]` the node announced, so a node that binds `0.0.0.0` announces an address a peer can actually dial. Because the port is a **field of the existing settings file** — additive, `SETTINGS_VERSION` unchanged — **no new persisted format and no new version-marker row is needed**.
- **[settled]** **The broadcast port is fixed: a protocol constant, not a setting.** A broadcast must reach a node that knows nothing yet, so it cannot itself be discovered; and a *configurable* broadcast port would let two nodes on one link silently fail to see each other. It is therefore one well-known UDP port, the same on every node; its **number** is the implementation's, documented where it is implemented.

**Frozen**: the two sources and their order; the entry being the `peers.json` shape; the table being a
source rather than an authority, with conflicts reported; startup / reconnect / change hand-down with a
generation; the hand-down being a §3.1 frame; the announcement being one signed datagram whose only
effect is to offer an address; "refresh an address, never introduce a key"; the room filter with its
default-deny; the peer port being configuration and needing no new format; and the broadcast port being
fixed. **Not frozen**: the announce interval and how long a node keeps announcing; how long a table may
be cached before the node asks again; the server's publication policy; the port **numbers** themselves;
and retries, back-off, or how a deployment enables broadcast at all.

## 5. Rooms

**A room is a membership list plus its rules**, and its landing place is `rooms.json` ([roadmap §4](roadmap-v1.0.md), [decisions §7](decisions.md)). §4.2 already froze the discovery **filter** that reads a room set; this section freezes the **file** the filter reads, and what a room *demands of* a message.

### 5.1 The file

- **[settled]** **One JSON file, `schema_version` first**, exactly as [`node.key`](connection.md) and `peers.json` do ([§2](#2-node-identity)):

  ```json
  {
    "schema_version": 1,
    "rooms": [
      {
        "name": "lab",
        "members": ["dev-a", "dev-b"],
        "rules": { "rate": { "messages": 60, "window_seconds": 60 },
                   "mention": "members",
                   "require_signature": true }
      }
    ]
  }
  ```

- **[settled]** **A member is a `node_id`** — the **device name** `peers.json` already keys on ([§2](#2-node-identity)), not a key and not an address. One fact, one home: **a room names who; `peers.json` says what a node is.** A member entry therefore carries no key, and adding one would be a second place a key could come from.
- **[settled]** **`rooms.json` and `peers.json` are two files, not one.** A `peers.json` entry's `rooms[]` is what that node *announces about itself* (§4.2); a room's `members[]` is what the **local deployer** says about who is in it. They are different claims from different authors, and neither overwrites the other: the announcement path only ever **refreshes an address** (§4.2), and the membership file only ever says **who may address the room** (§5.2).
- **[settled]** **A member that `peers.json` does not know is reported, not adopted** — the same rule §4.2 applies to announcements and [plugin-interface.md](plugin-interface.md) §6 to manifest sources. Membership never introduces a key, so it cannot introduce a trust either.

### 5.2 The rules

- **[settled]** **`rate` is `{messages, window_seconds}`, and it is per member.** Each member gets its own budget of messages per window, because a room-wide budget would let one member starve the others. Exceeding it is **`refused`** — [error-model.md](error-model.md) §4 already gives that category to "a policy the deployer set" and to a full queue, so **no new category is needed** and none is added.
- **[settled]** **`mention` is `"members"` or `"nobody"`**, and it governs `@` **from one member to another inside this room**. It **defaults to `"nobody"`**: a room that does not say otherwise forbids `@`, which is the default-deny the rest of the project uses ([security-model.md](security-model.md) §4). Addressing a node that is not a member is not a room act at all, and is refused by membership rather than by `mention`.
- **[settled]** **`require_signature` records [roadmap §4](roadmap-v1.0.md)'s third rule, and it cannot lower the floor §3 set.** §3 makes a signature **universal on the peer path** — verification step 2 rejects a frame whose signature does not verify — so in v1.0 `true` is the only value a file may carry and a `false` is **refused at load**. The field stays because the roadmap names it, and because a future version that wanted to allow unsigned traffic inside a room would have to say so here, in a decision, rather than by quietly setting a flag.
- **[settled]** **The rules are mechanism here and values there.** The kernel is what enforces a rate, a `@` permission and the signature floor; **the numbers and the choices are the deployer's** ([decisions §7](decisions.md): rate, rooms and `@` permissions are mechanism in the kernel and policy in the caller). Nothing in this section picks a default rate.
- **[settled]** **Membership is configuration, and v1.0 has no join protocol.** A room's members are what the deployer wrote in the file; there is no wire request to join, and a node cannot add itself. A dynamic membership protocol would be a new mechanism with its own authority question — the territory of [decisions §33](decisions.md) — so it is **not** invented here, and is left open below.

### 5.3 How the discovery filter reads it

- **[settled]** **"Configured for a room" means two things at once**: `rooms.json` **names** the room, **and** the room's `members[]` lists **this node's own `node_id`**. §4.2's filter adopts an announcement only when the rooms it names intersect the rooms the receiver is configured for — a room the node is merely *listed beside* is not one it is in, and it does not widen what the node will adopt.
- **[settled]** **The default-deny is concrete**: no `rooms.json`, an empty `rooms` array, or rooms that do not list this node all mean the node is configured for **no room**, so it **adopts no announcement** and works from the handed-down table alone (§4.1). Discovery is therefore usable before any room exists, which is what let §4.2 be frozen first.

**Frozen**: the file and its `schema_version`-first shape; a member being a `node_id`; `rooms.json` and
`peers.json` being two files with two authors and neither overwriting the other; an unknown member being
reported; the `rate` shape being per member and its refusal being `refused`; `mention`'s two values and
its `"nobody"` default; `require_signature` being unable to lower §3's floor; membership being
configuration with no join protocol; and the filter's reading of "configured for". **Not frozen**: the
rate **numbers** a deployment chooses; whether a room may have more rule kinds later; the wire flow a
future dynamic membership would use; how a member is *notified* that it was added; and anything about
cost metering ([decisions §7](decisions.md) names it beside rate, and it is not this section's).

## 6. The cross-region server

**A dedicated deployment of the same software, run by a deployer, that serves the traffic two nodes
cannot carry themselves** ([roadmap §4](roadmap-v1.0.md), [decisions §7](decisions.md), [§33](decisions.md)).
This section freezes **what it is and how a frame passes through it**; the digests it collects are §7's.

### 6.1 What it is, and who runs it

- **[settled]** **The project does not run it; a deployer does.** The repositories ship the software; nothing in them starts a server, offers an endpoint, or points at one the project operates. A node uses a cross-region server **because its own deployer configured one** (§6.4), and a deployment that configures none loses only the wide-area lane — every local rule in §2–§5 keeps working.
- **[settled]** **It is a deployment, not a second kind of process.** One code base, differentiated by deployment ([decisions §33](decisions.md)); what makes this one *dedicated* is that it is **its own machine and its own process, outside every workgroup** — not an in-network server moonlighting as a bridge ([roadmap §4](roadmap-v1.0.md)). Two roles, two deployments, one software.
- **[settled]** **One machine in v1.0.** Several are a commercialisation-layer item and are not part of this model ([roadmap §4](roadmap-v1.0.md), [decisions §33](decisions.md)); nothing in this section assumes a second instance exists, and nothing forbids one later.

### 6.2 The four roles

- **[settled]** **Signalling — "who is where".** It answers where a `node_id` can be reached and which network it is on, from what nodes have told it. It knows **addresses**, never payloads: a frame's `body` is not its business and it has no reason to be able to read one.
- **[settled]** **Relay — carrying what two nodes cannot carry themselves.** It forwards a frame it received to the `node_id` that frame names (§6.3). It carries bytes it cannot usefully change (§3.1 makes the frame byte-identical on both paths).
- **[settled]** **Management — the registry and the room definitions.** It may **publish** a node list and room definitions. **It is a source, not an authority**, and that is §4.1's rule applied one level out: a node merges what it is handed, its own `peers.json` and `rooms.json` stay authoritative for itself, and a conflict is **reported, never silently resolved**. A server is therefore never the place the truth lives — which is also what keeps this role from reading as "a service" (§6.5).
- **[settled]** **Audit aggregation — collecting the chain's digests.** This section fixes only the **role's shape and where it sits**: one of the four roles of this deployment, reached the same way as the others, holding digests rather than messages. **What a digest is and how it is batched is §7's** — written there, and authorised ([decisions §127](decisions.md)).

### 6.3 Routing and authorisation

- **[settled]** **A frame is relayed on its `to` field, and only on it.** The sender names the destination `node_id`; the server looks it up in the knowledge the signalling role maintains and hands the frame on. It **never opens, rewrites or answers for the body** — and it *cannot*: `to` sits inside the signature (§3), so a relay that altered where a frame goes would break the signature and the receiver would refuse it.
- **[settled]** **Who may ask: a node the server knows, with a signature that verifies.** The authorisation is the §3 model and nothing else — **no new credential and no new capability**: the sender must be a node whose `node_id` and public key the server holds (so step 1 of §3 succeeds), and the signature must verify (step 2). A server asked by a node it does not know refuses; it does not forward on the strength of an address.
- **[settled]** **It forwards only to someone it knows too.** A frame naming a destination the server cannot place is refused, not broadcast: a relay that sprayed a frame at every node it knew would turn one sender's mistake into everybody's traffic.
- **[settled]** **"Stateless" means about the content.** The server keeps **no message store** — it forwards and forgets, and it holds no authority over any payload. It does keep two things, and saying so is more honest than calling it memoryless: **who is where** (the signalling role's own data) and the **per-peer replay record** §3.2 gives every receiver. Both are about the transport; neither is a copy of anybody's history.
- **[settled]** **A direct connection takes the data path off the relay.** Once two nodes are talking directly, they stop handing frames for that pair to the server; what remains is the **management plane** — address knowledge and room definitions ([roadmap §4](roadmap-v1.0.md)). "Leaves the data path" therefore means exactly that: no further frames for that pair travel through the server, and nothing about the two nodes' own rules changes.
- **[settled]** **The server never dials a node.** It listens and waits to be dialled, which is why this project needs **no hole punching** ([roadmap §4](roadmap-v1.0.md), [decisions §7](decisions.md)): both sides dial **out**, so neither needs an inbound path through a NAT. §3.1 already fixes a sender's behaviour — try the peer's own addresses, and hand the frame to the relay when that fails; this is the half that says the relay is standing there ready to receive it.

### 6.4 Where a node learns its address

- **[settled]** **The cross-region server is a peer, and it is configured like one.** Its `node_id` and public key live in the node's own `peers.json` ([§2](#2-node-identity), [decisions §13](decisions.md)), which is what makes §3's verification work in both directions, and its addresses are that entry's `addresses[]`.
- **[settled]** **The node's network settings name which peer is its cross-region server.** That is the policy half: *whether* to use one, and *which* one, is the deployer's choice, so it is a field of the settings section the node already has — **additive, `SETTINGS_VERSION` unchanged, and therefore no new persisted format and no new version-marker row**.
- **[settled]** **Discovery still knows nothing about it.** §4.2's rule stands: no announcement carries a cross-region server, no handed-down table offers one, and a node that has none simply has no wide-area lane.

### 6.5 Does this read as the project operating a service?

**[settled]** **No, and the reasons are checkable.** [roadmap §1](roadmap-v1.0.md) forbids an officially
operated service, and this is the section that could have broken it:

- **The runner is stated**: §6.1 says a deployer runs it, and the project ships software rather than a service.
- **There is no project endpoint**: nothing in this section names an address, a hostname or a default. A node reaches a server only because its own deployer put one in its `peers.json` and its settings.
- **The roles are mechanism**: route, publish, collect — all four are acts on the deployer's own machines, and every *choice* they carry (whether to use a server, which one, what to publish) is the deployer's ([decisions §7](decisions.md)).
- **The one role that could look like a service is held to a source, not an authority** (§6.2), so the server is never where the truth lives: a deployer can run one, stop it, or never have one, and every node still holds its own history and its own membership.
- **No credential is issued by it**: §6.3 authorises by signature, so a server grants no access and hands out no secret; a deployment that runs one can take it away again without reissuing anything.

The same test applies to §4.1's in-network server, and it passes there for the same reasons.

**Frozen**: that a deployer runs it and the project does not; that it is a dedicated deployment of the
same software, one machine in v1.0; the four roles and what each does and does not know; routing on the
signed `to` and nothing else; authorisation being the §3 model (a known `node_id` with a verifying
signature) and **not** a new credential or capability; a relay forwarding only to a destination it knows;
what "stateless" does and does not mean; a direct connection taking the data path off the relay; the
server never dialling a node; the configuration shape being a `peers.json` entry plus a settings field
with no new format; the red-line answer of §6.5; the **reporting half** of the same model — §6.6's
registration, heartbeat and online-status table, with its two directions kept apart; and §6.7's **liveness
judgement** — a fact about reachability, unanimous among the witnesses that remain, and never a removal.
**Not frozen**: the
routing algorithm's data structure and the server's **capacity limits**; how a deployment publishes its
address to its own users; what the aggregation role does with a digest beyond holding the latest one
(that is §7's to say, and written there); how several servers
would be run together (a commercialisation-layer item); the **numbers** §6.6 freezes for v1.0 (15 s and
45 s are defaults of the kind §3.1's timeouts are); and where §6.6's table is kept.

### 6.6 Reporting upward: registration and heartbeat

**[settled]** **A node tells its server it is here, and goes on telling it.** §6.2's four roles are what a
server *does*; this subsection is the other direction — **what a node reports**. It is one mechanism at two
levels: a node to the server it was configured with, and an in-network server to the cross-region server
above it. Nothing here is a new frame *type*: a registration is an ordinary §3 frame addressed to the
server, and so is a heartbeat.

**[settled]** **The registration frame** — sent when the node connects, from a node the server already
knows (§6.3's authorisation):

```json
{
  "v": 1, "from": "dev-a", "to": "<the server's node_id>", "ts": 1737970000000,
  "body": {
    "register": 1,
    "addresses": ["10.0.0.7:47821"],
    "capabilities": [],
    "rooms": ["lab"]
  },
  "sig": "…"
}
```

- **`from` is who is speaking**, and it is what the server binds the row to: the identity is in the
  preamble, not in the body, so a registration cannot name somebody else.
- **`addresses`** is where the node says it can be dialled — empty is legal, and is what a node reachable
  only through the relay reports. **`capabilities`** and **`rooms`** are what the node *claims about
  itself*: the same fields `peers.json` carries, with the same standing — **a claim, not a fact**.
- **No key travels in a registration.** The server already holds the node's public key, because it is in
  the server's own `peers.json` — which is what made step 1 of §3 succeed. A key arrives through
  configuration and never through a frame, so §4.2's rule ("an announcement refreshes an address, it cannot
  introduce a key") holds here **by construction**: a node the server does not know cannot register at all.
- The server verifies it the way it verifies anything — §3's six steps: it knows the sender, the signature
  verifies, `v` is spoken, `to` is itself, `ts` is inside the window, and it is not a replay — and then
  records the row below. A registration is **idempotent**: sending it twice is not an error and the second
  one changes nothing.

**[settled]** **The heartbeat frame** — the smallest thing a node can say:

```json
{ "v": 1, "from": "dev-a", "to": "<the server's node_id>", "ts": 1737970015000,
  "body": { "heartbeat": 1 }, "sig": "…" }
```

- **Every 15 seconds**, and that number is a *default*, not a law. A beat that is lost costs nothing: the
  next one arrives 15 seconds later and refreshes the row.
- **Why 15 s**: it is comfortably inside §3.2's backward window (five minutes), so a delayed beat is still
  a beat and never a `stale` frame; it is not so fast that a fleet's beats become the busiest thing on a
  link; and it is three times smaller than the offline threshold below, which is what lets that threshold
  ride out one lost beat and one slow round trip.
- **The heartbeat is not the session opener.** `{"hello": 1}` binds a socket (the transport's business); a
  heartbeat is a **signalling** fact — it says a node is still *there*, which is a different question from
  whether a socket is open, and a node may hold a session open and still be gone. Coupling the two would
  make the transport answer a presence question, and the liveness judgement that reads this table
  (V-proto-2) must not inherit that.

**[settled]** **The online-status table** — what the server keeps, per node it knows:

| field | meaning |
|---|---|
| `node_id` | the key, and the row's only identity |
| `addresses[]` | the addresses the node **last reported** (empty when it reported none) |
| `last_heartbeat_ms` | when the last heartbeat arrived — or the registration, which counts as one |
| `state` | `online` while `now − last_heartbeat_ms ≤ 45 s`, and `offline` after that |

- **The offline rule is the ratio, and 45 s is v1.0's value**: three missed intervals. Both are written
  down here so the liveness work has a number rather than a guess.
- **A row is created by a registration, refreshed by a heartbeat, and never deleted by going offline.**
  Deleting it would make "offline" indistinguishable from "never registered", and *who was here* is
  exactly what the liveness judgement needs. A restart forgets the whole table — the same trade §3.2's
  record makes — and a node re-registers when it reconnects.
- **The table is the signalling role's data** (§6.2): where a node is, right now. It holds **no payload**,
  and nothing in it is a message.
- **The claims are a source, and the files stay authorities.** What a node reports feeds the registry a
  server may publish (§6.2's management) **as a source**: the deployer's `peers.json` and `rooms.json`
  remain authoritative for their own node, and a disagreement is **reported, never silently resolved** —
  §4.1's and §6.2's rule, unchanged. A node cannot become known, or gain a key, by registering.
- **[settled]** **The row also keeps what the node claimed.** Beside the four fields above, a row carries the
  claims its registration made — `capabilities` and `rooms` — because §6.7 reads the sibling set out of
  them (an in-network server declares the `"server"` claim; §6.7 says how, and what it does and does not
  grant). They are claims like every other: a **source** and never an authority, and a node that lists
  something it does not hold gains nothing by listing it.

**[settled]** **One mechanism, two levels.** The frames above are the same when a node talks to its
in-network server and when an in-network server talks to the cross-region server above it: same §3 frames,
same table, same numbers. What differs is only *who is the node* and *who keeps the table* — the same
kernel, differentiated by deployment (§6.1). One shape, so the two levels cannot drift apart.

**[settled]** **An in-network server registers as *itself*, not as itself plus its nodes.** Its frame names
its own `node_id` and reports its own addresses; the nodes below it are **not** disclosed upward. Three
reasons, and the first is structural:

- **A key cannot arrive by frame.** The cross-region server must hold the in-network server's key in *its*
  `peers.json` for step 1 of §3 to succeed. If the frame also named the nodes below, the top server would be
  expected to know identities that arrived through a message rather than through configuration — the thing
  §4.2 and §9 forbid — so it could not verify a single one of them.
- **The addresses below are not reachable from above.** A node behind an in-network server has a LAN
  address; publishing it to a remote peer is a topology leak with nothing on the other side of it. What
  makes the cascade work is that the in-network server's **own** address is reachable: **one public endpoint
  is enough**, which is the model this section is built on.
- **Identity comes from configuration here as everywhere.** If a deployer wants the top server to know the
  nodes below, they go in *that* server's `peers.json` — out of band, by the deployer, which is also how a
  new node joins at all.

**[settled]** **A new node joins by configuration, not by asking.** An administrator adds the node to the
server's `peers.json` **before** it can register; there is **no automatic approval**, and no request a node
can send that makes itself known. That is §5.2's rule ("membership is configuration; v1.0 has no join
protocol") applied to the server's node list — and the place a *dynamic* join would be decided is
[decisions §33](decisions.md)'s territory, put to M6/M7 rather than settled here.

**Frozen**: that a node registers and then heartbeats; both being ordinary §3 frames addressed to the
server and verified by §3's six steps; the registration body's members (`register`, `addresses`,
`capabilities`, `rooms`) and that **no key travels in a frame**; the heartbeat's body (`heartbeat`) and that
it is distinct from the session opener; the online-status table's fields — the four (`node_id`, `addresses[]`,
`last_heartbeat_ms`, `state`) and the claims a registration added (`capabilities`, `rooms`) — and that a row is never
deleted by going offline; the offline rule (three missed intervals) and v1.0's numbers (15 s, 45 s); the two
levels sharing one shape; an in-network server registering as itself; and joining being configuration with
no automatic approval. **Not frozen**: the numbers themselves (15 s and 45 s are v1.0's defaults, of the kind
§3.1's timeouts are); where the table is kept (memory-only is this section's reading of §6.3, not a format);
how a server *publishes* what it holds (that is §6.2's management, unchanged); what a deployment does about
a peer that has been judged gone (§6.7 — the judgement is frozen there; the acting is not); and whether
anything ever prunes the table.

### 6.7 Liveness: who decides a node is gone

**[settled]** **§6.6 gives every node a *report*; this subsection gives the deployment a *judgement*.** They
are different facts and must not be confused. A row that has gone `offline` says **the server stopped
hearing from a node** — one observer, one silence. A **judgement** says **everybody who could still reach it
has said they cannot**, which is the only statement strong enough to act on. Like §6.6, nothing here is a new
frame type: a probe is an ordinary §3 frame, and so is a report.

**[settled]** **Who judges what, and at which level.** Two levels, the shapes of §6.6:

- **A node's peers are its own workgroup** — the peers that share its in-network server, which is why the
  server's own knowledge *is* the membership ([roadmap §4](roadmap-v1.0.md): a workgroup is one LAN plus its
  server). A node asks about **its peers**; the in-network server judges **the nodes it knows**.
- **A server's peers are its sibling servers** — the other in-network servers registered with the same
  cross-region server. **A server says so in its registration**: an in-network server declares the ordinary
  claim **`"server"`** in the `capabilities` list of its §6.6 registration (the row keeps it, §6.6), and
  the cross-region server's siblings are **the rows whose claims include `"server"`**. A sibling asks about
  **its siblings**; the cross-region server judges **the servers it knows**.

**[settled]** **`"server"` is a claim, not a capability.** It is an ordinary string in a registration's
`capabilities` list — the same claim list §6.6 already carries — and **not** a member of the control plane's
capability vocabulary ([decisions §83](decisions.md)): declaring it grants nothing, adds no word to any list
and lets a node do no act it could not do before (§3's signature is authentication; authority is the
capability model's, [security-model.md §4](security-model.md)). What it buys is exactly one thing: a node that
declares it is **probed as a sibling** and, if it stops answering, judged by the siblings' unanimity. A node
that declares it falsely only invites probes it will not answer, and the worst that follows is a judgement
about itself, which the rules below already govern.

**[settled]** **The probe — a question with an answer, direct first.**

```json
{ "v": 1, "from": "dev-a", "to": "dev-b", "ts": 1737970030000,
  "body": { "probe": 1 }, "sig": "…" }
```

- The peer answers **`{"alive": 1}`**, addressed back and signed by the peer — so what a prober gets is
  **evidence about the peer's key-holder and its clock**, not merely an open socket. A wedged process still
  accepts a connection; only an answer says the node behind it is working.
- **Direct first, and the relay when that fails** — §3.1's ordinary rule, unchanged. That is also what lets a
  probe cross a NAT: a probe is a frame like any other.
- **Every 15 seconds** — the same interval as §6.6's heartbeat — and **three consecutive unanswered probes
  (45 s)** is what makes a prober conclude **"I cannot reach it"**. The numbers are §6.6's on purpose: the
  node's own evidence and the server's table have to cross the same line at the same moment, or the two
  halves of the judgement would disagree about when 45 seconds have passed.
- What a prober keeps is **its own view** — per peer, when the last answer arrived, how many probes in a row
  went unanswered, and whether it currently holds the peer *reachable* or *unreachable*. It is **runtime
  state**: memory-only, like the server's table (§6.6) and §3.2's record — and it is written to **no chain**,
  because a suspicion is not an event (the last bullet below says why).

**[settled]** **The report: what a prober tells its server, and how often.**

```json
{ "v": 1, "from": "dev-a", "to": "<server>", "ts": 1737970030000,
  "body": { "unreachable": "dev-b" }, "sig": "…" }
```

- **`{"unreachable": "<node_id>"}`** when the view turns unreachable, and **`{"reachable": "<node_id>"}`**
  when it turns back. A prober **repeats the report every probe cycle (15 s) while its view stands**, so a
  report is a *pulse* rather than a one-off: a witness that goes quiet stops being a witness.
- A report counts **only while it is fresh** — inside the same 45 s — so a stale "I cannot reach it" cannot
  outlive the view that produced it.

**[settled]** **The threshold: unanimity among the witnesses that remain.** A node is judged gone when
**(1)** there is at least one **witness** — another node this server knows, itself reachable right now, and
not the subject — and **(2)** every witness has reported it unreachable, freshly.

- **A witness of life vetoes.** One node that can reach the subject is proof it is *alive*, and a failure has
  innocent explanations (a route that broke, a blocked port, a full backlog) while a success has none. So a
  failure is never proof and a success always is: the rule can only be unanimity among failures.
- **A majority would be wrong exactly where it matters.** In a partition, half a workgroup can reach the
  subject and half cannot; a majority would declare a **live** node gone, which is the worst error this
  mechanism can make. Unanimity claims no more than "those of us who are here cannot reach it", which is
  true.
- **The witness set is what keeps the rule from being vacuous**, and it is why the question is not "how many
  reported". A node that is itself down cannot report; a node that is itself unreachable must not be counted
  as a witness, because it cannot testify. So the threshold is unanimity **among those still able to speak** —
  and a node alone in its workgroup is **never** judged, because nobody can testify: the honest answer.
- **The judgement is about reachability and touches no identity.** A judged node stays in `peers.json`, stays
  in the server's table and keeps its key (§6.6: a row is never deleted). Nothing is revoked by being judged
  gone.

**[settled]** **An in-network server's own loss is confirmed by its siblings, not by the nodes below it.** A
server that stops being reachable is judged by the **other in-network servers**, reporting to the
cross-region server above them — and the reason the nodes below cannot do it is physical: **they share a LAN
and a power feed with their server, so they die with it.** A silence that includes the witnesses is not
evidence.

- **How a sibling knows**: it probes the siblings it knows — the servers that declared `"server"` to the
  server above them (§6.6), which a prober finds in **its own `peers.json`**, where their public keys are;
  a key arrives through configuration and never by frame (§4.2), so the same file is the sibling set and
  its keys — with the same probe and the same numbers; a sibling behind a NAT is reached through the
  cross-region server's relay, because a probe is a §3.1 frame and the relay is what carries a frame a
  direct path cannot.
- **How it reports**: the same two bodies, addressed to the cross-region server instead of an in-network
  one. There is no second vocabulary for the second level.
- **How the cross-region server judges**: the same rule, unchanged — unanimity among the sibling witnesses
  that remain. It **aggregates rather than probes**, and that is not a choice: §6.3's rule that the server
  never dials applies to it too, so its own evidence is what it already has (the heartbeats of §6.6) plus
  what its siblings tell it.

**[settled]** **After the judgement: record it, and leave the acting to the deployer.**

- **The row gains one field**, `judged_at_ms`: the moment the threshold first held, cleared when the node is
  heard from again. §6.6's four fields keep their meaning **unchanged** — `state` is still `offline` from
  silence alone, and `judged_at_ms` is the stronger, collective fact. They are kept apart on purpose: a node
  can be `offline` because its heartbeats stopped while its peers still reach it, which is a different
  situation from one nobody can reach.
- **Two audit names, one per transition** (the vocabulary's additions, recorded in [decisions §102](decisions.md)):
  **`host.connection.peer_offline`** when the judgement is reached, with `{peer, witnesses, reports}`, and
  **`host.connection.peer_recovered`** when the node is heard from again, with `{peer, method}` — `method`
  being `heartbeat` or `probe`. Both are written by the **judging server**, the one holding the table: at the
  node level by the in-network server, at the server level by the cross-region server.
- **A prober writes no row.** Its view is runtime state and a suspicion is not a fact: recording "I cannot
  reach X" per node would make one partition write *"X is gone"* into half the chains — evidence this design
  explicitly refuses to trust. The chain records the **judgement**, the only statement strong enough to act
  on.
- **The protocol defines no removal.** There is no automatic kick, no ejection frame, and no rule that a
  judged node is dropped: the kernel provides the mechanism — the judgement, the row, the event — and **what
  a deployment does about it is the deployer's policy** (kick it, ignore it, page a human). That is
  [roadmap §1](roadmap-v1.0.md)'s red line: the kernel gives mechanism, not policy. The deployer's tools
  already exist and are unchanged — membership is configuration (§5.2), a node joins by being put in
  `peers.json` (§6.6), and it leaves the same way.

**[settled]** **Recovery is being heard from, not a re-admission.** A judged node that comes back is heard
from — a heartbeat or an answered probe clears `judged_at_ms`, returns `state` to `online`, and writes
`host.connection.peer_recovered`. Nothing is restored because nothing was taken: identity was never revoked
and the row was never deleted (§6.6). A node that restarts re-registers (§6.6, idempotent) and its row is the
one it had.

**Frozen**: that a judgement is a separate fact from §6.6's `offline`; the scope at each level (a node's
workgroup; a server's siblings); how a sibling **says** it is one (the registration's `"server"` claim,
which is a claim and not a capability); the probe (`{"probe": 1}` / `{"alive": 1}`, direct first then relay, every
15 s, three misses = the prober's own *unreachable*); the report (`{"unreachable": …}` / `{"reachable": …}`,
repeated each cycle while the view stands, counted only while fresh); the threshold (**unanimity among the
witnesses that remain**, a witness of life vetoing, the subject never judging itself); that a solo node is
never judged; that the judgement touches no identity, no membership and no key; that a server's own loss is
confirmed by its **siblings**, and why the nodes below cannot do it; that the judgement is recorded in the
judging server's row (`judged_at_ms`) and in two audit names; that a prober writes no row; that the protocol
defines **no removal**; and that recovery is being heard from rather than re-admitted. **Not frozen**: the
probe's transport timeouts and retransmission details; whether probes are staggered so a large workgroup does
not probe in lockstep; the freshness number (45 s is v1.0's, §6.6's); whether a **room** may narrow the peer
scope later (it is a policy grouping, not a transport one); how a deployment acts after a judgement and the
shape of any kick API (V-3 or later); whether reports are batched; and how a **partition** is resolved at the
deployment level — that is [decisions §33](decisions.md)'s suppression machinery, and it is **§6.8** below.

### 6.8 Suppression: the three layers before a stand-in

**[settled]** **Nothing fires on one node's suspicion** ([decisions §33](decisions.md), authorised by [decisions
§127](decisions.md) point 4). Before a node may stand in for its centre it must pass **three layers, all
required**:

1. **A waiting period** — this node cannot reach the centre and silently retries for **60 s** (§33's 30 s –
   2 min, at the default). It is **longer than §6.6's 45 s online window on purpose**: a node waiting while the
   centre would still call it online is waiting on a fact the other side has not reached.
2. **Global confirmation** — the wait elapsed, and the group agrees the centre is gone. This is **§6.7's rule
   applied to the centre**: unanimity among the witnesses that remain, with a witness of life vetoing. §6.7 is
   where the reason is argued — *a majority would be wrong exactly where it matters*.
3. **Backoff plus precedence** — the first in line waits a **random backoff inside 0–30 s** and **stands down
   the moment it sees a takeover broadcast**. "First in line" is **`node_id` order** for v1.0 (no
   registration-frame field), and the backoff is derived from `(node_id, now)`, so two nodes that confirm in
   the same millisecond still draw different delays.

**[settled]** **The centre, for these layers**, is the **in-network server** this node knows — the peer that
declares the server claim (§6.6/§6.7) and is not the cross-region server above it. It is a `node_id` like any
other, which is what lets §6.7's table decide about it.

**[settled]** **The machine is local, and it is not in the chain.** Its phase (`candidate` / `waiting` /
`confirming` / `backing-off` / `standing-in`) lives in memory, like §6.6's table and §6.7's records: a
suspicion is not an event.

**[settled]** **The reports travel between peers** (M5-3b-1). A node sends its neighbours the same
`{"unreachable": …}` / `{"reachable": …}` §6.7 sends upward — addressed **sideways**, because the centre is
the node that is not answering — and each node keeps **its own** witness table with the **centre** as its
subject, so §6.7's rule decides locally. It is one table with two subjects (a peer's reachability upward,
the centre's here), and it asks the cross-region server for nothing: §6's four roles are unchanged.

**[settled]** **The takeover broadcast is its own body**, and it too travels only between peers:
`{ "takeover": 1, "centre": <node_id>, "by": <node_id>, "at_ms": <ms> }`. A node that hears one **stands
down** — §33's third layer, in one frame.

**[settled]** **Standing in is an act, and it is three things** (M5-3b-2). Past the backoff, and only if it
is **first in line**, a node: **sends** the takeover broadcast to **each** of its peers; opens a
**temporary segment** on its own chain (the `segments` row and its `segment_opened` event — the segment's
*file* is later work); and records the act as **`host.connection.takeover_declared`**, whose detail names
the segment, the centre, the node and the moment. A node that hears a broadcast while it is standing in
records **`host.connection.stood_down`**. Both rows are **new names in the `host.connection.*` family**, and
neither is one of the chain's stream events. **The chain keeps being written as the node always wrote it**:
§33 folds the temporary segment back into the main chain later, so a stand-in diverts no write at all
(owner's point 4).

**Not here yet.** The **return** — a centre that comes back, and the segment folding into the main chain —
is **M5-3c**. Until then a temporary segment stays open, which is the honest state.

## 7. Audit digests — M4e-1

**[settled]** **A digest is a commitment to a point on the chain**, and nothing more: the chain's
**head hash** and the **number of events** that lead to it. It is *not* a second hash and *not* the whole
chain — it is what a verifier can compare against, and it is read off the store the chain already lives in
(`audit`'s last hash and its event count). **The chain's formula does not move**: this section extends what
*travels*, never `compute_hash` or `verify_chain` ([decisions §127](decisions.md) point 2).

**[settled]** **The shape on the wire** is an ordinary §3 frame — a signed message addressed to the
server itself, the same mechanism §6.6's registration and heartbeat use — whose body is:

```json
{
  "digest": 1,
  "chain": "…64 hex characters, or null on an empty chain…",
  "length": 1024
}
```

- **`chain`** is the head hash, lowercase hex, or `null` when the chain is empty; **`length`** is the event
  count, `0` then. Both facts are reported, never suppressed.
- **The identity is the preamble's `from`** (§3), so the body names nobody: like a heartbeat, a digest is a
  **statement**, and the server answers it with nothing.

**[settled]** **The server holds the latest digest per node** — in **memory**, like the session table and
the replay record. §6.2's aggregation role "holds digests rather than messages": nothing here becomes a
second copy of anyone's history, and nothing about it is written to the server's own chain.

**[settled]** **Batched on a 30-second timer, and that number is a default.** A node reports every 30
seconds; the interval is the implementation's to configure, in the same sense §6.6's 15 s is. The timer
**reads this node's own chain and reports it** — it writes nothing.

**Not here yet — M4e-2.** A **key event** (an ejection, a fork, a temporary centre's takeover) is pushed
the moment it happens rather than waiting for the batch ([roadmap §4](roadmap-v1.0.md)). The immediate
path is a later batch, and two of its three triggers — the fork and the takeover — arrive with M5.

**Not M4's.** The temporary centre, `provisional` and `fork` are M5/M6.

**[settled]** **The segment semantics are not here.** What a temporary segment *is*, the cross-segment
reference at its head, and the questions roadmap §7 still leaves open are
[docs/audit-v2.md](audit-v2.md) — the audit subsystem's specification. This section is only the digest's
transport: a commitment to a point, reported upward.

**Frozen**: a digest is the head hash and the event count; the wire shape above; that the server holds the
latest per node in memory; the 30-second batch as a *default*; and that the timer reads and never writes.
**Not frozen**: what the aggregation role does with a digest beyond holding the latest one; how several
servers would be run together (a commercialisation-layer item).

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
