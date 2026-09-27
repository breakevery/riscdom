[中文](security-model.zh-CN.md) | English

# The security model

**Status** v1.0 specification (milestone [M1](roadmap-v1.0.md)) ｜ **Date** 2026-09-27 ｜ **Baseline**
v0.9.9 (`3365970`) ｜ **Audience** administrators — the people who run a node and answer for it. §5
(reporting a vulnerability) is addressed to whoever finds one.

**What this document is.** The fifth and sixth things the [v1.0 roadmap](roadmap-v1.0.md) §6 says must be
on disk before the freeze: **credentials and key management**, and the **disclosure policy**. It states
where every secret lives, what a capability is worth, and what the kernel is *not* responsible for. It is
the design behind [`SECURITY.md`](../SECURITY.md) (the short public statement) and
[`docs/control-plane-api.md`](control-plane-api.md) §3 (the wire).

## 1. Credentials and keys

- **[settled]** **Two kinds of secret, two algorithms.** Nodes identify themselves with **Ed25519**;
  API access uses a **bearer token**. Nothing here is home-grown: Ed25519 in a standard encoding
  (PEM or JWK), and a token that is 32 bytes of OS randomness in hex ([`server/src/token.rs`](../server/src/token.rs)).
- **[settled]** **Private keys are files by default, the keyring optionally.** A node's private key lives
  in `<data-dir>/node.key` with mode `600` (on Windows, an owner-only ACL), or in the OS keyring when the
  operator prefers it. Which one is a deployment choice, and [decisions §13](decisions.md) fixed this
  shape before any of it was implemented.
- **[settled]** **Rotation runs several keys in parallel.** A node may publish a new key while the old
  one is still valid: during the grey period both verify, so a rotation does not need a flag day.
- **[settled]** **Revocation is network-wide and loud.** A revocation list is broadcast to the room, and
  a compromised node is ejected everywhere rather than in the places that happened to notice
  (decisions §13).
- **[settled]** **Bulk import is JSON, one object per node**: `{node_id, addresses[], public_key,
  capabilities, rooms[]}`. It is a *config* file (`peers.json` in the roadmap's §4) — it holds public
  keys, never private ones.

## 2. Where each secret lives

**[settled]** Four secrets, four homes. The rule behind the table: **a secret never sits in a file that
is meant to be read, backed up or screenshotted**, and a non-secret preference never sits in the keyring.

| Secret | What it is | Where it lives | Lifetime |
|---|---|---|---|
| **Node identity** (v1.0) | the Ed25519 key pair that signs this node's messages | `<data-dir>/node.key`, mode 600 / owner-only ACL, or the OS keyring — the operator's choice | the node's; rotated in parallel, revoked by broadcast |
| **API token** | the bearer token the control plane demands | `<data-dir>/token`, one hex line, `600` or an owner-only ACL. Minted on the server's **first start**; `--no-auth` is the documented way to run without one | until the operator deletes the file (a new one is minted on the next start) |
| **LLM key** | the model provider's key | the **OS keyring** — service `com.breakevery.riscdom`, account `llm-api-key:<provider>`. Adopted from `DEEPSEEK_API_KEY` into **memory** at startup and never written to the keyring automatically | until the operator replaces it; a keyring that refuses degrades to memory, so it may not survive a restart — the caller is told what the keyring said |
| **Remote node token** | another node's API token, used by *Settings → Network* | the **OS keyring**, account `remote-token:<host>` — per host, so a second node cannot overwrite the first | until "disconnect and use this machine", which deletes it |

- **[settled]** **`settings.json` holds no secret, and never has.** The file's own rule since v0.4 is
  that only non-secret preferences live there; the LLM key went to the keyring, and the remote-token
  field was **deleted** before it ever shipped writing one ([decisions §70](decisions.md)).
- **[settled]** **Reading a credential never creates one.** The settings page may *show* the LAN token
  through a read-only command that reads `<data-dir>/token` and refuses cleanly when it is absent
  ([decisions §67](decisions.md)); minting stays the server's job. Opening a page must not bring a
  credential into existence.
- **[settled]** **A token that cannot be protected stops the server.** If the platform will not restrict
  the token file to its owner, the server refuses to start rather than serve a token another account can
  read ([`server/src/token.rs`](../server/src/token.rs)).
- **[settled]** **Secrets are never logged, echoed or put on a command line.** The value goes to the
  caller and nowhere else — not to the audit chain, not to a message, not to an argument.

## 3. The transport boundary

- **[settled]** **The open-source build speaks plaintext HTTP and installs an authentication hook.**
  That is the whole of it: `Authorization: Bearer <token>`, checked by the `Authn` hook, with the
  capability decision made in the request path ([decisions §6](decisions.md)).
- **[settled]** **TLS is the deployment's responsibility, and the boundary is written down rather than
  assumed.** A deployer who needs confidentiality puts the node behind a TLS terminator; this project
  does not ship one, does not pretend the traffic is encrypted, and says so in this document and in
  [`control-plane-api.md`](control-plane-api.md).
- **[settled]** **The board is loopback until somebody says otherwise.** The desktop's embedded server
  binds `127.0.0.1` unless the allow-LAN switch is on, and that switch carries a warning the moment it is
  ticked — because turning it on means every device on the network can reach the node, and only the token
  stands in the way.
- **[settled]** **Zero new dependencies for the protocol.** HTTP for commands and queries, SSE for the
  event stream, no WebSocket — one direction, no handshake to get wrong.

## 4. Capabilities

- **[settled]** **A capability is a precondition, and it is a type.** The route table's third column *is*
  the `Capability` value, so a route that skips the check cannot be written ([decisions §6](decisions.md));
  the vocabulary lives in [`server/src/auth.rs`](../server/src/auth.rs) and holds **32** entries today.
- **[settled]** **Who holds what is the deployment's business.** The open-source build offers the
  mechanism — every endpoint has a capability, every actor is checked — and the mapping from people to
  capabilities is the deployer's policy. This is red line 1 again: the kernel does not decide who is
  allowed to do what, it only makes the question answerable.
- **[settled]** **Deciding a request is a capability of its own.** `POST /v0/sandboxes/requests` may be
  made by anyone who may run an agent (`agent.run`); *deciding* one needs `sandbox.read` plus the
  capability the request's own action implies ([decisions §36](decisions.md)). Asking and granting are
  different powers and the vocabulary says so.
- **[settled]** **Local and remote are different positions, and the eight wiring commands know it.** The
  eight names that wire a node act on **this** machine in every mode ([decisions §71](decisions.md)) —
  a window whose server is unreachable must still be able to stop being that window — and a browser
  looking at a node, local or remote, is a **read-only board**: it can look, it cannot rewire
  ([decisions §62](decisions.md)).
- **[settled]** **A plugin's powers are its declared capabilities.** The sandbox plugin interface reuses
  this model rather than inventing a second one ([decisions §3](decisions.md)).

## 5. Disclosure policy

- **[settled]** **Channel.** Report privately through a **GitHub Security Advisory**; never a public
  issue. [`SECURITY.md`](../SECURITY.md) is the short form a reporter reads first, and this section is
  what it points at.
- **[settled]** **Timeline.** **48 hours** to acknowledge a report, **7 days** to give a first
  assessment (severity, affected versions, whether a fix is being written), and **90 days** as the
  coordinated-disclosure horizon — a report becomes public after 90 days or when a fix ships, whichever
  comes first.
- **[settled]** **CVE.** The maintainers request a CVE identifier for a confirmed, exploitable
  vulnerability; the advisory is published with it.
- **[settled]** **No bug bounty.** There is **no** monetary reward program. This is a non-profit,
  Apache-2.0 project: saying so plainly is kinder than leaving a reporter to infer it.
- **[settled]** **Credit by default, anonymity on request.** A reporter is named in the advisory unless
  they ask not to be.
- **[settled]** **An embargo is not a promise about somebody else's release.** If a fix cannot land in
  90 days, the report is published anyway and the mitigations are stated.

## 6. Threat model: what the kernel is responsible for

**Responsible for**

- **[settled]** The sandbox: an executor's code runs in a VM, and the guest does not reach the host's
  filesystem or network except through the interfaces the host offers it.
- **[settled]** The audit chain: append-only, hash-linked, and not rewritable or prunable through SQL
  (the `BEFORE UPDATE` / `BEFORE DELETE` triggers in [`audit/src/store.rs`](../audit/src/store.rs) are
  the hard guarantee).
- **[settled]** The credential rules of §1–§2: no secret in a settings file, no secret in the chain, no
  credential minted by looking at a page.
- **[settled]** The capability check on every endpoint: no route that can skip it.

**Not responsible for**

- **[settled]** The network's confidentiality (§3): plaintext HTTP is the open-source build's honest
  position, and TLS belongs to the deployment.
- **[settled]** A model provider's behaviour, or what an AI decides to do with the capabilities it was
  given. The kernel records what happened; it does not supervise.
- **[settled]** A hostile *operator*: someone who already holds the machine's account can read the token
  file, the keyring and the data directory. This model protects against other accounts, other devices and
  other nodes — not against the person who owns the box.
- **[settled]** Supply-chain trust beyond what is pinned: QEMU is never bundled or downloaded
  ([decisions §32](decisions.md)), and the RISC-V toolchain is pinned and hashed.

## 7. The audit chain is the root of trust

- **[settled]** **The chain is the product.** Every event carries the hash of the one before it
  (`prev_hash`) and its own `hash`, which is unique in the table; UPDATE and DELETE are refused by
  triggers, so the log cannot be rewritten or pruned through SQL.
- **[settled]** **Its guarantees do not move.** audit v2 extends the *semantics* to "main chain +
  temporary segments" and adds metadata at a segment's head **without changing the hash formula**
  ([decisions §33](decisions.md)) — and because that extension touches red line 5, it needs its own
  authorisation (`PROJECT_CONSTITUTION.md` §8: *when in doubt, ask first*).
- **[settled]** **The chain holds no secret.** `agent.llm.request` records a hash and token counts;
  keyring events record the `provider_id` only ([`SECURITY.md`](../SECURITY.md)).

## 8. What this document does not cover

- **[settled]** **Signing and notarization of the packages.** The installers and packages are unsigned
  (SmartScreen on Windows, Gatekeeper on macOS); that is a commercialisation-layer item and it is stated
  in the release notes, not here.
- **[settled]** **A multi-tenant deployment.** One node serves one operator's work; isolation between
  tenants is not a claim this model makes.
- **[open]** **The revocation list's transport.** §1 says a revocation is broadcast to the room; how that
  broadcast reaches a node that is currently offline is part of the connection layer's work (§4 of the
  roadmap) and is not decided here.
- **[open]** **Whether `node.key` may live in the keyring in every deployment.** The keyring is optional
  by design, and a headless server without a keyring must fall back to the file — the fallback's
  conditions are settled when the key lands.
