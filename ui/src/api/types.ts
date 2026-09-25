/**
 * Every shape the backend hands the UI (v0.9 D2b-1).
 *
 * These used to live in `api/tauri.ts`, which made the *shapes* a property of one
 * transport. They are the control plane's shapes: the desktop's Tauri commands
 * and the Web client's HTTP endpoints both answer with them, so they live here
 * and both implementations import them
 * (`docs/control-plane-api.md` §5 names the matching view types on the host
 * side).
 */

export interface ChainStatus {
  status: "Intact" | "Broken";
  length?: number;
  at_id?: number;
  reason?: string;
}

export interface AuditStatus {
  count: number;
  chain: ChainStatus;
  /** Whether the audit-failure alert is on (v0.8). */
  alert_on_failure: boolean;
  /** Audit writes that failed and have not been shown yet (v0.8). */
  failures: string[];
}

export interface LlmStatus {
  configured: boolean;
  provider_id: string;
  base_url: string;
  model: string;
  persisted: boolean;
}

/** A selectable LLM provider preset (pure data from the host). */
export interface ProviderPreset {
  id: string;
  display_name: string;
  base_url: string;
  default_model: string;
  requires_key: boolean;
  is_local: boolean;
}

export interface AuditEvent {
  id: number;
  timestamp_ms: number;
  actor: string;
  action: string;
  detail: unknown;
  prev_hash: string;
  hash: string;
  // v0.8: which agent caused the event; null on rows written before it.
  agent_id?: string | null;
}

export interface AgentOutcomeView {
  kind: "final" | "max_iterations" | "failed";
  content?: string | null;
  reason?: string | null;
  iterations: number;
}

/** One run from the host's derived index (read-only, v0.4 1d). */
export interface RunView {
  run_id: string;
  status: string;
  /** Full configuration digest (64 hex characters). */
  fingerprint: string;
  /** First 16 hex characters, for display. */
  fingerprint_short: string;
  parent_run_id: string | null;
  session_id: string | null;
  /** The snapshot this run was restored from, or null (v0.5 batch 3). */
  resumed_from_snapshot: string | null;
  started_at_ms: number;
  ended_at_ms: number | null;
}

/** Environment preflight (v0.4 batch 3). */
export interface PreflightRow {
  step: string;
  /** `ok` / `failed` / `not_run`. */
  state: string;
  detail: string | null;
}

export interface PreflightView {
  ran: boolean;
  fingerprint: string;
  checked: boolean;
  ok: boolean;
  rows: PreflightRow[];
  failed_step: string | null;
  detail: string | null;
  suggestion: string | null;
  checked_at_ms: number | null;
  overridden: boolean;
}

/** Whether the LLM is ready, and why not. */
export interface LlmReadiness {
  ready: boolean;
  reason: string | null;
  suggestion: string | null;
}

/** A locally-detected OpenAI-compatible provider. */
export interface LocalProviderInfo {
  id: string;
  display_name: string;
  base_url: string;
  models: string[];
}

export interface LocalProbeResult {
  found: boolean;
  providers: LocalProviderInfo[];
  probed: string[];
}

/**
 * One top-level field of two runs' fingerprints (v0.6 batch 1). `a` / `b` are the
 * documents' values as the host read them off the chain — whole nested objects,
 * not their keys. They are structured JSON, so the UI renders them, never
 * re-parses a string the host formatted for it.
 */
export interface FingerprintFieldDiff {
  field: string;
  a: unknown;
  b: unknown;
  is_different: boolean;
}

/** A snapshot on disk. */
export interface SnapshotMeta {
  name: string;
  size_bytes: number;
  created_at_ms: number;
  /** "tcp-relay" (real) or "reboot-fallback". */
  mode: string;
}

/** VM status for the top-bar badge (v0.3 #4c). */
export interface VmStatus {
  running: boolean;
  since_ms: number | null;
}

/** One-click toolchain download (mirrors the host `DownloadEvent`).
 *
 * The tag is `state` since v0.9: the same flattened shape the envelope's payload
 * carries, so the event and the polling status agree.
 */
export type ToolchainDownloadEvent =
  | { state: "started"; total_bytes: number | null }
  | { state: "progress"; downloaded: number; total: number | null }
  | { state: "verifying" }
  | { state: "extracting" }
  | { state: "done"; install_path: string }
  | { state: "failed"; reason: string }
  | { state: "cancelled" };

export interface ToolchainDownloadStatus {
  in_progress: boolean;
  last_event: ToolchainDownloadEvent | null;
}

/** QEMU status (v0.3 5b-2): same shape as the toolchain view. */
export interface QemuView {
  found: boolean;
  path: string | null;
  /** "EnvVar" | "KnownPath" | "Path" | "Manual" */
  source: string;
  /** Full search record (where we looked and what happened). */
  diagnostics: string;
}

/** RISC-V GCC toolchain status. */
export interface ToolchainView {
  found: boolean;
  path: string | null;
  /** "EnvVar" | "KnownPath" | "Path" | "Manual" */
  source: string;
  /** Full search record (where we looked and what happened). */
  diagnostics: string;
}

/** A persisted session summary. */
export interface SessionMeta {
  id: string;
  title: string;
  created_at_ms: number;
  updated_at_ms: number;
  message_count: number;
}

/** One persisted message. */
export interface SessionMessage {
  id: number;
  session_id: string;
  role: string;
  content: string;
  tool_call_json: string | null;
  tool_call_id: string | null;
  created_at_ms: number;
}

export interface SessionDetail {
  meta: SessionMeta;
  messages: SessionMessage[];
}

/** The host event envelope — re-exported so `api.HostEnvelope` keeps working. */
export type { HostEnvelope } from "./envelope.ts";

/**
 * `/v0/health` — the cheapest authenticated call (v0.9 D2b-2).
 *
 * The desktop never asks: its host is in the same process, so "is it reachable and
 * am I allowed" is not a question there. The Web client uses it to check a token.
 */
export interface HealthView {
  status: string;
  version: string;
  uptime_ms: number;
}

/**
 * `/v0/status` — what this node is doing (v0.9 D2b-2).
 *
 * `agents` is `1` on purpose today: the host instance answering *is* the only agent
 * the control plane knows until the executor roster is wired, so the panel says
 * exactly that rather than leaving the number to be misread.
 */
export interface StatusView {
  status: string;
  version: string;
  uptime_ms: number;
  connections: number;
  sse_subscribers: number;
  agents: number;
  agent_id: string;
}

/**
 * One executor this node can dispatch to (`/v0/executors`, v0.9 D2b-4b).
 *
 * The list is the fleet from `settings.json` — the labels a task's `target` may name.
 */
export interface ExecutorView {
  agent_id: string;
}

/** The fleet, as served. */
export interface ExecutorListResponse {
  executors: ExecutorView[];
}

/**
 * One sandbox definition, as served — the fields of `host-core`'s `SandboxView`
 * (`sandbox_def.rs`), whose serialised names are the ones below.
 */
export interface SandboxView {
  name: string;
  display_name: string | null;
  memory_mb: number | null;
  qemu_exe: string | null;
  toolchain_path: string | null;
  kernel: string | null;
  notes: string | null;
  /** `"manual"` (hand-written) or `"discovered"` (a scan found it). */
  source: string;
  /** Could this definition run **right now**? */
  runnable: boolean;
  /** Does a hand-written definition use this name? The scanned entry stays, marked. */
  shadowed: boolean;
}

/** `/v0/sandboxes`: every definition, plus which one a run would use. */
export interface SandboxListResponse {
  sandboxes: SandboxView[];
  current: string | null;
  default: string;
}

/** `/v0/sandboxes/current`. */
export interface CurrentSandboxResponse {
  current: string | null;
  default: string;
}

/** One installed resource the scan found (`/v0/sandboxes/candidates`). */
export interface CandidateView {
  /** `"toolchain"` or `"qemu"`. */
  kind: string;
  /** The version directory it was found under. */
  version: string;
  path: string;
  /** `"installed"` (under the data directory) or `"system"`. */
  origin: string;
  /** Does it run (`--version` exits 0)? */
  runnable: boolean;
}

/**
 * The node's network wiring (v0.9.9 内网接入).
 *
 * Two directions, one shape, because they are configured on one screen: **out**
 * (this desktop connects to an in-network RiscDom server) and **in** (this
 * desktop serves its own board to the network). Mirrors
 * `host_core::settings::NetworkSettings`.
 */
export interface NetworkSettings {
  /** The server to connect to (`"out"`); `null` keeps the embedded host. */
  remote_url: string | null;
  /**
   * No token here — deliberately (v0.9.9 `"out"`). A remote server's bearer token
   * is a credential and lives in the OS keyring under `remote-token:<host>`; a
   * settings file stays readable without handing anyone access to another node.
   *
   * The input on the network page is that write, and `save_remote_token` performs
   * it — this shape has no field for it to land in.
   */
  /** Serve this node's board to the network (`"in"`). Off keeps it in this window. */
  lan_enabled: boolean;
  /** Where the embedded server binds; `null` means loopback (`127.0.0.1:7821`). */
  lan_bind: string | null;
  /** Bind on every interface rather than loopback — the switch that reaches a phone. */
  lan_allow_lan: boolean;
}

/** What the node's board is doing right now (v0.9.9 内网接入). */
export interface LanStatus {
  /** Is the embedded server running? */
  running: boolean;
  /** The address it bound, when it is running (`0.0.0.0:7821` for a LAN board). */
  bound: string | null;
  /** This machine's own address on the network — what a phone has to type. */
  address: string | null;
  /** Why it is not running, when the settings asked for it and it would not start. */
  problem: string | null;
}
