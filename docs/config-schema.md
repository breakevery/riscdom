[中文](config-schema.zh-CN.md) | English

# Configuration schema

**Status** v1.0 specification (M7f) ｜ **Date** 2026-09-29 ｜ **Audience** distribution integrators and
tool authors: anyone who writes, validates or generates a node's configuration files.

**What this document is.** [decisions §16](decisions.md) settles that the configuration is described as a
JSON Schema, in this file. This is that description as **documentation**: the three files a node's data
directory holds — `settings.json`, `peers.json`, `rooms.json` — with every field, its type, whether it may
be absent, what an absent one means, and how each format is versioned. A machine-readable `*.schema.json`
is **not shipped yet**; when one is, it is generated from this description rather than kept beside it by
hand.

**Scope: what is written to disk, not the runtime API.** The control plane's HTTP surface is
[control-plane-api.md](control-plane-api.md); the wire protocol is [connection.md](connection.md). Nothing
here describes a request, a response or a frame.

## 1. settings.json

`<data-dir>/settings.json`. Read at start-up, written by the host when a setting changes. One object with
no wrapper: its fields are the ones `LocalSettings` carries
([`host-core/src/settings.rs`](../host-core/src/settings.rs)).

| Field | Type | May be absent | When absent | Meaning |
|---|---|---|---|---|
| `version` | number | yes (reads as the **oldest** format) | the oldest format, v1 | The format's version. Written as `2` from v1.0 M2b-1; a `version` that is not a number is not a settings document at all. |
| `toolchain_path` | string \| null | yes | `null` | A manual RISC-V GCC path; `null` means discovery. An empty or whitespace-only string is treated as absent. |
| `zig_path` | string \| null | yes | `null` | A manual Zig executable; `null` means discovery. |
| `rust_sysroot` | string \| null | yes | `null` | A manual `rust-std-*` sysroot **directory**; `null` means the environment. |
| `qemu_path` | string \| null | yes | `null` | A manual QEMU executable; `null` means discovery. |
| `preflight` | object \| null (**PreflightCache**, §1.1) | yes | `null` | The last environment preflight, bound to the fingerprint it was produced for. |
| `theme` | `"light"` \| `"dark"` \| `"system"` \| null | yes | `null` = `system` | The interface theme. |
| `language` | `"system"` \| `"en"` \| `"zh"` \| null | yes | `null` = `system` | The interface language. |
| `alert_on_audit_failure` | boolean | yes | **`true`** | Whether the interface shouts when an audit write fails. The log line and the `audit:failed` event are sent either way. |
| `sandboxes` | array of **SandboxDef** (§1.2) | yes | `[]` | Definitions written by hand. The scan's findings never land here: the registry is merged on read. |
| `default_sandbox` | string \| null | yes | `null` | The definition a caller gets when it names none; `null` = the built-in fallback. |
| `executors` | array of **Executor** (§1.3) | yes | `[]` | The executor processes this node dispatches to. An empty list is a working configuration: `POST /v0/tasks` then answers `404` for every target. |
| `network` | object \| null (**NetworkSettings**, §1.4) | yes | `null` | How this node talks to the network. `null` = no network wiring has been configured. |
| `llm_configs` | object of **LlmConfigEntry** (§1.5) | yes | `{}` | Each executor's model configuration, keyed by **executor id**. Never carries a key: credentials live in the OS keyring. |

**Every field is additive.** A file written before a field existed loads with that field's default, and
`version` does not move for an addition — which is why most of this table is "may be absent, and here is
what that means".

### 1.1 `preflight` (PreflightCache)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `fingerprint` | string | no | — |
| `ok` | boolean | no | — |
| `failed_step` | string \| null | yes | `null`: nothing failed (present only when `ok` is false) |
| `detail` | string \| null | yes | `null`: no detail |
| `suggestion` | string \| null | yes | `null` |
| `checked_at_ms` | number | no | — |
| `overridden` | boolean | yes | `false`: the user did not choose "continue anyway" |

### 1.2 `sandboxes[]` (SandboxDef)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `name` | string | no | — what a request names and what the merge matches on |
| `display_name` | string \| null | yes | `null` |
| `memory_mb` | number \| null | yes | `null` = the host's default (128 MiB) |
| `qemu_exe` | string \| null | yes | `null` = discovery |
| `toolchain_path` | string \| null | yes | `null` = discovery |
| `kernel` | string \| null | yes | `null`: the model picks the ELF in `start_vm` |
| `notes` | string \| null | yes | `null` |
| `supports_multiplexing` | boolean | yes | **`false`** — a declaration that this definition may host several instances at once |

### 1.3 `executors[]` (Executor)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `label` | string | no | — the identity a task addresses (`Task.target`) |
| `program` | string | no | — |
| `args` | array of string | yes | `[]` |

A half-specified executor (an empty `label` or an empty `program`) is **dropped on load**, not repaired.

### 1.4 `network` (NetworkSettings)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `remote_url` | string \| null | yes | `null` = the embedded host, the behaviour of every version before this field |
| `lan_enabled` | boolean | yes | `false` = the node serves nobody |
| `lan_bind` | string \| null | yes | `null` = loopback (`127.0.0.1:7821`) |
| `lan_allow_lan` | boolean | yes | `false` = loopback only |
| `cross_region_server` | string \| null | yes | `null` = no wide-area lane. A `node_id` that must appear in this node's `peers.json`. |
| `server_role` | object \| null (**ServerRoleSettings**, §1.4.1) | yes | `null` = this node is only a client |

**No secret is in this section.** A remote server's bearer token lives in the OS keyring, and a key never
appears in a settings file — the rule `settings.json` has followed since v0.4.

#### 1.4.1 `network.server_role` (ServerRoleSettings)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `bind` | string | **no** | — a deployer writes the address. There is deliberately no default: a default would be the project naming where a server is. |

### 1.5 `llm_configs` (values: LlmConfigEntry)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `provider_id` | string | yes | `""` |
| `base_url` | string | yes | `""` |
| `model` | string | yes | `""` |

**The key is not here.** `llm_configs` holds the non-secret half a restart needs; the API key lives in the
OS keyring. A settings file can be read, copied or backed up without leaking access to a model.

## 2. peers.json

`<data-dir>/peers.json`. Who this node knows. One object with the version marker **first**, then the
entries.

| Field | Type | May be absent | When absent | Meaning |
|---|---|---|---|---|
| `schema_version` | number | yes (reads as `1`) | `1` | The format's version: **1** since v1.0 M4a. |
| `peers` | array of **PeerEntry** (§2.1) | yes | `[]` | The entries. A node may know nobody, and that is a working configuration. |

### 2.1 `peers[]` (PeerEntry)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `node_id` | string | no | — the device name a signed message's `from` carries |
| `addresses` | array of string | yes | `[]` — a node reachable only through the relay reports none |
| `public_key` | object: a **public** JWK (`OKP`/`Ed25519`) | no | — a `d` member is **refused**: a private key where a public one belongs is an error, not a value |
| `capabilities` | array of string | yes | `[]` — what the node *claims* it may do; a claim, not a fact |
| `rooms` | array of string | yes | `[]` — what the node *claims* about its rooms; membership is the local `rooms.json` |

## 3. rooms.json

`<data-dir>/rooms.json`. Membership and the rules a room carries.

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `schema_version` | number | yes (reads as `1`) | `1` — the format's version since v1.0 M4c |
| `rooms` | array of **Room** (§3.1) | yes | `[]` |

### 3.1 `rooms[]` (Room)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `name` | string | no | — |
| `members` | array of string (**`node_id`s**) | yes | `[]` |
| `rules` | object (**RoomRules**, §3.2) | no | — |

### 3.2 `rooms[].rules` (RoomRules)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `rate` | object (**RateRule**, §3.3) | no | — |
| `mention` | `"members"` \| `"nobody"` | yes | **`"nobody"`** — default deny is the project's habit |
| `require_signature` | boolean | no (and must be **`true`**) | — a `false` is **refused on load**, because §3 makes a signature universal on the peer path and a room setting must not lower that floor |

### 3.3 `rooms[].rules.rate` (RateRule)

| Field | Type | May be absent | When absent |
|---|---|---|---|
| `messages` | number | no | — how many messages one member may send inside one window |
| `window_seconds` | number | no | — the window, in seconds |

The budget is **per member**, never per room: a room-wide budget would let one member starve the others.

## 4. Versioning

The rule is [api-compatibility.md §6](api-compatibility.md): **each persisted format carries its version
marker in its first field**, and the markers are **independent per format** — they are not one
repository-wide number:

| Format | Marker | Value today |
|---|---|---|
| `settings.json` | `version` | **2** (v1.0 M2b-1; the first real migration, 1 → 2) |
| `peers.json` | `schema_version` | **1** (v1.0 M4a) |
| `rooms.json` | `schema_version` | **1** (v1.0 M4c) |

- **New reads old: allowed.** The reader migrates on open, and a settings file gets a
  `settings.json.bak` beside it before it is rewritten, so a migration that goes wrong has one known way
  back.
- **Old reads new: refused.** A file from a newer build answers `data_too_new` and nothing is read,
  applied or written — never a partial read, never a silent downgrade.
- **An addition does not move the marker.** A new optional field is additive: an older file loads with the
  field's default. Only a *structural* change moves the number, and then a migration step is written down
  in the same batch that moves it.

## 5. What is not covered

- **`node.key` is identity, not configuration.** It is an Ed25519 JWK with its own `schema_version` = 1
  ([connection.md](connection.md) §2), listed in [api-compatibility.md §6](api-compatibility.md)'s marker
  table like every persisted format — and deliberately **not** part of this schema: a validator for
  configuration must never have to handle key material.
- **The SQLite stores** (`audit.db`, `sessions.db`) carry their marker in `PRAGMA user_version`; the
  chain's schema and the session store's are their own business, not configuration.
- **The token file** (`<data-dir>/token`) has no version at all: one line of hex, shape-checked.
- **Runtime state is never configuration.** The online table, the witness table, sessions, the run index
  and a VM's live state are what a node *observes*, not what a deployer *writes*. None of it appears in
  the three files above, and none of it is a schema this document could freeze.
