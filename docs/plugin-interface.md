[中文](plugin-interface.zh-CN.md) | English

# The sandbox plugin interface

**Status** v1.0 specification (milestone [M3](roadmap-v1.0.md)) ｜ **Date** 2026-09-28 ｜ **Baseline**
v0.9.9 (`3365970`) ｜ **Audience** plugin authors — whoever writes a sandbox this kernel can run.

**What this document is.** The interface [roadmap §8](roadmap-v1.0.md) describes, written down so it
can be **frozen** — [decisions §3](decisions.md) requires the freeze to come before the kernel API's,
because an interface reshaped after the cross-device work would be work done twice. It is a
specification: the kernel's own implementation of it is v1.x work ([decisions §3](decisions.md)),
and so is everything this document marks *not frozen*.

**Its companions.** [decisions §3](decisions.md) (the sandbox is a plugin),
[§4](decisions.md) (preset environments) and [§5](decisions.md) (the repositories) are the decisions
behind it; [security-model.md](security-model.md) §4 is where a capability's worth is stated;
[error-model.md](error-model.md) is the vocabulary an error travels in;
[api-compatibility.md](api-compatibility.md) is what "frozen" means for anything on this surface.

Each section below ends with what it **freezes** and what it **leaves open**.

## 1. What is frozen, and what is not

- **[settled]** **Frozen here**: the transport (§2), the mechanism layer's four operations and their
  frame grammar (§3), the semantics layer's two operations (§4), the *framework* of the capability
  declaration (§5), the manifest's required keys (§6), the error and version rules (§7), the
  architecture-independence constraint (§8) and the trust model (§9).
- **[open]** **Not frozen here**: the **capability declaration format's details** (§5 — draft by
  design, the one part [roadmap §8](roadmap-v1.0.md) says should be frozen last), the **architecture
  abstraction** (§8 — the requirement is frozen, the trait is not designed), and every *name* a
  plugin-defined payload carries.

**Frozen**: the list above. **Not frozen**: the two marked items, and any shape this document does
not state in so many words — silence is not a promise.

## 2. The transport

- **[settled]** **Out of process, over stdio, one JSON object per line** — the shape
  [`worker`](../worker/README.md) already uses, and the reason [decisions §3](decisions.md) calls the
  plugin "isomorphic with `worker`". No in-process plugin, no dynamic library, no ABI.
- **[settled]** **stdout is the protocol channel.** Exactly one **response** line per **request**
  line, nothing else on it. A plugin that writes anything else to stdout has broken the protocol.
- **[settled]** **Events go to stderr**, in the envelope the rest of the project already uses
  (`{"version", "kind": "event", "event", "agent_id", "task_id", "ts", "payload"}` —
  [control-plane-events.md](control-plane-events.md)). A plugin's own events use the `plugin:` prefix,
  the way `worker` prefixes itself with `worker:`. Events are diagnostic: a supervisor parses all
  lines one way and ignores what it does not know.
- **[settled]** **One request at a time.** The kernel sends a request and waits for its response
  before sending the next; a response carries the `id` of the request it answers.
- **[settled]** **The process lives for the session.** Unlike `worker` (one task, then exit), a plugin
  is started once and answers requests until the kernel closes stdin. On stdin EOF it releases what it
  holds and exits `0`. A plugin that exits or dies before EOF is a **crash** to the kernel
  ([§7](#7-errors-and-version-negotiation)).

**Frozen**: the channel split, the one-line-per-message rule, the event envelope, serial dispatch and
the session lifetime. **Not frozen**: how a plugin is *supervised* (restart policy, resource limits) —
that is the kernel's business, and not part of what a plugin author implements.

## 3. The mechanism layer

**[settled]** Four operations are **mandatory**: a plugin that cannot answer all four is not a sandbox
plugin. Every frame is one line; `v` is the protocol version ([§7](#7-errors-and-version-negotiation))
and `id` is the request's identity.

```text
request   {"v":1,"id":"<string>","op":"<name>","args":{...}}
response  {"v":1,"id":"<same string>","ok":true,"result":{...}}
          {"v":1,"id":"<same string>","ok":false,"error":{"kind":"<string>","message":"<string>","retryable":<bool>}}
```

### 3.1 `start`
- **args** `{"instance":"<id>","definition":"<name|absent>","workspace":"<path>","data_dir":"<path>","instance_dir":"<path>","shared_dir":"<path>"}` — the directories are the two [decisions §4](decisions.md) persistent directories, handed to the plugin because *what is mounted* is the plugin's decision.
- **result** `{"state":"running","since_ms":<int>}`
- **already running** is `ok:false`, `error.kind:"refused"` — refused, never queued or restarted under the caller (the rule the VM already follows).

### 3.2 `stop`
- **args** `{"force":<bool|absent>}`
- **result** `{"state":"stopped","since_ms":<int>}`
- **nothing is running** is `ok:true`: stopping twice is not an error.

### 3.3 `execute`
- **args** `{"input":"<string>","timeout_ms":<int|absent>}`
- **result** `{"outcome":<any JSON>,"output_ref":<string|absent>}` — the guest's answer. Its shape is the **plugin's**; the kernel treats `outcome` as opaque JSON. `output_ref`, when present, is a handle the kernel may pass back to `output` (§3.4) so a large answer need not travel twice.
- **timeout** is the plugin's to enforce. Exceeding it is `ok:false`, `error.kind:"network"` with `error.message` naming the deadline ([error-model.md §4](error-model.md) — the retry verdict for a timeout).
- **execute does not imply start**: calling it with nothing running is `ok:false`, `error.kind:"invalid"`, and the kernel's answer is the caller's problem, not a silent start.

### 3.4 `output`
- **args** `{"since":<int|absent>,"ref":<string|absent>}` — `since` is the index the plugin returned last time; `ref` is an `output_ref` from `execute`.
- **result** `{"lines":["<string>",...],"next":<int>}` — the accumulated console output, in order. An index from the future or a ref the plugin does not know is `ok:false`, `error.kind:"invalid"`.

**Frozen**: the four operation names, the request/response grammar, the argument and result keys of
§3.1–§3.4, and the two rules that a second `start` is refused and a second `stop` is not. **Not
frozen**: the *contents* of `input` and `outcome` (architecture-dependent by design, §8), and anything
about how the plugin runs a guest.

## 4. The semantics layer

**[settled]** Two operations are **optional**. A plugin **declares** which it has ([§5](#5-the-capability-declaration));
calling an operation a plugin did not declare is `ok:false`, `error.kind:"invalid"` — the kernel does
not infer it, and does not fall back to another plugin.

```text
snapshot      {"name":"<string>"}          -> {"name":"<string>","mode":"<string>"}
fingerprint   {}                           -> {"fingerprint":"<string>","schema":"<string>"}
```

- **[settled]** `snapshot`'s `mode` is the plugin's own word for how the snapshot was taken; the kernel carries it through rather than interpreting it (the existing two modes are QEMU's, and are not part of this interface).
- **[settled]** `fingerprint` answers the same kind of thing a run's fingerprint does ([run-provenance.md](run-provenance.md)): a stable string plus the schema that produced it, so two nodes can compare *what a configuration is* without comparing opaque bytes.

**Frozen**: the two operation names, their frames, and the rule that an undeclared operation is
refused. **Not frozen**: the `mode` vocabulary and what a fingerprint is computed over.

## 5. The capability declaration

**[default]** — **this is the one part of the interface deliberately left as a draft**, because it is
the part plugin authors implement first and the part most likely to need a revision after one real
plugin exists ([roadmap §8](roadmap-v1.0.md); the format was previously said to live "in §8" while §8
said "in §14.11", which is a circle this section breaks by stating the framework and naming the rest
as draft).

**The framework — frozen:**

- **[settled]** **Where it is written**: the manifest ([§6](#6-the-manifest)), key `capabilities`, a list of **strings**.
- **[settled]** **Who checks it**: the **kernel**, never the plugin.
- **[settled]** **When it is checked**: at **registration** (the plugin is read and its declaration validated before it is ever started) and again at **request** time (an operation whose capability was not declared is refused, even if the plugin offers it).
- **[settled]** **Unknown names are refused, not ignored.** A capability the kernel cannot check is a capability nobody grants; a declaration carrying one is `error.kind:"invalid"` and the plugin does not load. The names come from the kernel's own vocabulary (the `Capability` list, [control-plane-api.md §3](control-plane-api.md)) — not from the plugin.
- **[settled]** **An empty declaration is not "everything".** A plugin that declares nothing can do nothing; the default is deny, exactly as it is for every other actor ([security-model.md](security-model.md) §4).

**Draft — not frozen:** the exact spelling of a declaration entry (a bare name versus a name with a
scope), whether the mechanism and semantics operations require *named* capabilities of their own (for
example a `sandbox.execute`-shaped name) or are granted by the ones the kernel already has, and
whether a declaration may be narrowed per instance. **These are settled before v1.x implements anything**;
until then a plugin author should treat this section as the shape of the file and expect the
spelling to move.

## 6. The manifest

- **[settled]** **A manifest is one JSON file** beside the plugin, `plugin.json`. It is the first thing the kernel reads; a manifest that does not parse is a refusal, never a default.
- **[settled]** **Required keys**: `name`, `version`, `protocol` (the `v` this plugin speaks), `entry` (`{"program":"<path|name>","args":["<string>",...]}` — the command the kernel starts), `capabilities` ([§5](#5-the-capability-declaration)).
- **[settled]** **Optional keys**: `description`, `persistent_dirs` (`{"instance":<bool>,"shared":<bool>}` — which of [decisions §4](decisions.md)'s two directories this plugin wants).
- **[settled]** **Manifest sources are merged, not exclusive** — plugin declaration, kernel scan and developer-written files, exactly the three [decisions §4](decisions.md) names; a conflict is reported, never silently resolved by precedence.
- **[settled]** **Preset content carries a hash and is verified at start-up** ([decisions §4](decisions.md)); a plugin whose content does not match what was installed is refused.

**Frozen**: the file name, the required and optional keys above, and the two rules (no silent
defaults, no silent precedence). **Not frozen**: where a plugin is installed *from* (the repository
format is [decisions §5](decisions.md), built in v1.x), and the signature's carrier — the package
reuses the credential spec, and that spec's shape is not this document's to state.

## 7. Errors and version negotiation

- **[settled]** **An error is a response, not a crash.** A plugin that cannot do what it was asked answers `ok:false` with `error.kind` from [error-model.md](error-model.md)'s categories (`network`, `refused`, `crashed`, `partial`, `invalid`), a human `message`, and the `retryable` verdict that model assigns. The kernel maps it onto its own `DispatchError` — it does not invent a sixth category.
- **[settled]** **A process that dies is a crash**: the kernel reports it as `crashed` with the exit status, `retryable: false` for the same task ([error-model.md](error-model.md)).
- **[settled]** **`v` is checked, both ways.** A kernel refuses to use a plugin whose `protocol` major it does not speak; a plugin that sees a request with a **newer** `v` answers `ok:false`, `error.kind:"invalid"` and keeps serving nothing it does not understand. This is [api-compatibility.md](api-compatibility.md) §6's "an old reader never reads a newer format" applied to a process instead of a file.
- **[settled]** **Additive within a major**: a new `op`, a new optional key, a new event — all may arrive in a minor release. Removing an operation, or changing what one means, needs a new major ([api-compatibility.md](api-compatibility.md) §2, §3).

**Frozen**: the error frame, the category vocabulary it borrows, the crash rule, and the version rule
both ways. **Not frozen**: the kernel's restart policy after a crash, and anything about how a
*newer* kernel talks to an older plugin beyond the refusal rule.

## 8. Architecture independence

- **[settled]** **The kernel may not assume the guest is RISC-V.** Plugin content is not constrained by architecture, and nothing in this interface — no frame, no key, no operation — names a machine, an instruction set or an emulator ([roadmap §8](roadmap-v1.0.md), [decisions §2](decisions.md)). RISC-V stays the substrate and the default implementation; it is not a requirement of the interface.
- **[settled]** **The proof is in the frames**: `execute`'s `input` is a string and its `outcome` is opaque JSON (§3.3), and `snapshot`'s `mode` is the plugin's word (§4). A kernel that could not run a non-RISC-V plugin through these four operations would have added an assumption this document forbids.
- **[open]** **The abstraction's trait is not designed here.** *How* the kernel holds a plugin it cannot make assumptions about — the in-process seam behind these operations — is v1.x work, and freezing a trait before one plugin exists would repeat the mistake §5 is avoiding.

**Frozen**: the constraint (no architecture assumption, RISC-V as default not as requirement), and the
frame shapes that make it checkable. **Not frozen**: the trait, the module, and any in-process type a
future implementation introduces.

## 9. Trust model

- **[settled]** **A plugin's powers are the capabilities it declares** — checked the way every other capability is checked, inside the same model ([decisions §3](decisions.md), [security-model.md](security-model.md) §4). This interface adds no second permission system.
- **[settled]** **Out of process is a boundary, not a formality.** The plugin is not in the kernel's address space; a plugin that wants something it did not declare must ask for it through a request, where the kernel can refuse.
- **[settled]** **What the kernel does on a plugin's behalf is on the chain.** A start, a stop, an execute that the kernel caused is an audit row like any other act — the interface never writes to the chain itself ([security-model.md](security-model.md) §7), and it must not become a second log.
- **[settled]** **The plugin is not trusted with the kernel's secrets.** No key, token or credential travels in a request frame or an event payload; the directories in §3.1's `start` are the only state handed over.

**Frozen**: the capability-constrained trust model, the process boundary, "the chain is written by the
kernel, not by the plugin", and the no-secrets rule. **Not frozen**: sandboxing the plugin process
itself (what it may touch on the host) — a deployment and OS concern this interface does not decide.

## 10. The red lines

[roadmap §1](roadmap-v1.0.md) states four constraints and calls them the test every milestone's work
has to survive. This is how this interface survives them; the constraints themselves are not repeated
here, only cited.

- **Not a general-purpose sandbox.** The interface exists so the sandbox **is** pluggable: §8 forbids any frame from naming a machine, and RISC-V is a default a plugin may replace, never a requirement a plugin must meet. A reader may therefore take this document as the reason the constraint is still true.
- **No built-in supervisor.** Every frame here is a **mechanism** a caller drives; none of them decides anything. There is no operation for "do the right thing", and §3.3 refuses to start implicitly — a decision is not something this interface can make.
- **No officially operated service.** A plugin is a file and a process on the caller's own machine; §6 says where it is read from and what is verified, and states no party that serves it.
- **The audit invariants do not move.** §9 says the chain is written by the kernel, not by the plugin, and this interface defines **no** audit event and no hash input — it borrows the vocabulary that exists rather than adding to it.

**Frozen**: nothing in this document may be amended in a way that fails one of the four. **Not
frozen**: the statements above are the *test*, not a licence to change what the four mean.
