// All Tauri `invoke` calls live here, so every backend call is reviewable in
// one place. Never pass secrets anywhere except `setLlmConfig`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ChainStatus {
  status: "Intact" | "Broken";
  length?: number;
  at_id?: number;
  reason?: string;
}

export interface AuditStatus {
  count: number;
  chain: ChainStatus;
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
}

export interface AgentOutcomeView {
  kind: "final" | "max_iterations" | "failed";
  content?: string | null;
  reason?: string | null;
  iterations: number;
}

export const getAuditStatus = () => invoke<AuditStatus>("get_audit_status");

export const listAuditEvents = (
  limit: number,
  actor?: string,
  actionPrefix?: string,
) =>
  invoke<AuditEvent[]>("list_audit_events", {
    limit,
    actor: actor ?? null,
    actionPrefix: actionPrefix ?? null,
  });

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

export const listRuns = (limit = 20) =>
  invoke<RunView[]>("list_runs", { limit });

export const getRun = (runId: string) =>
  invoke<RunView | null>("get_run", { runId });

/** UI theme preference (v0.4 #11a): `light` / `dark` / `system`. */
export const getTheme = () => invoke<string>("get_theme");

export const setTheme = (theme: string) => invoke<void>("set_theme", { theme });

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

export const preflightStatus = () =>
  invoke<PreflightView>("preflight_status");

export const runPreflight = () => invoke<void>("run_preflight");

export const acknowledgePreflight = () =>
  invoke<PreflightView>("acknowledge_preflight");

export const setLlmConfig = (
  apiKey: string,
  baseUrl: string,
  model: string,
  providerId: string,
  remember: boolean,
) => invoke<void>("set_llm_config", { apiKey, baseUrl, model, providerId, remember });

/** Does a key for `providerId` exist in the OS keyring? (never returns the key) */
export const hasStoredKey = (providerId: string) =>
  invoke<boolean>("has_stored_key", { providerId });

/** Load a stored key from the OS keyring into host memory. */
export const loadStoredKey = (providerId: string) =>
  invoke<void>("load_stored_key", { providerId });

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

export const getLlmReadiness = () =>
  invoke<LlmReadiness>("get_llm_readiness");

export const probeLocalLlm = () =>
  invoke<LocalProbeResult>("probe_local_llm");

export const getProviderPresets = () =>
  invoke<ProviderPreset[]>("get_provider_presets");

export const clearLlmConfig = () => invoke<void>("clear_llm_config");

export const getLlmConfigStatus = () =>
  invoke<LlmStatus>("get_llm_config_status");

export const runAgent = (userInput: string) =>
  invoke<AgentOutcomeView>("run_agent", { userInput });

export const getWorkspaceFiles = () => invoke<string[]>("get_workspace_files");

export const readWorkspaceFile = (path: string) =>
  invoke<string>("read_workspace_file", { path });

/**
 * The AI workspace root, as an absolute path (v0.5 batch 2). The audit export
 * builds its default file name under it instead of hard-coding a location.
 */
export const getWorkspaceRoot = () => invoke<string>("workspace_root");

export const getSerialBuffer = () => invoke<string>("get_serial_buffer");

export const exportSerialLog = (path: string) =>
  invoke<number>("export_serial_log", { path });

export const exportAuditJsonl = (path: string) =>
  invoke<number>("export_audit_jsonl", { path });

/**
 * Export **one run's** audit interval as JSONL (v0.5 batch 1). Returns the number
 * of events written. The host writes only inside the workspace, and refuses a run
 * that has not ended.
 */
export const exportRunAudit = (runId: string, path: string) =>
  invoke<number>("export_run_audit", { runId, path });

/** A snapshot on disk. */
export interface SnapshotMeta {
  name: string;
  size_bytes: number;
  created_at_ms: number;
  /** "tcp-relay" (real) or "reboot-fallback". */
  mode: string;
}

export const listSnapshots = () => invoke<SnapshotMeta[]>("list_snapshots");

export const deleteSnapshot = (name: string) =>
  invoke<boolean>("delete_snapshot", { name });

/** Save a real (tcp-relay) snapshot of the host-owned VM. Returns bytes. */
export const saveSnapshotReal = (name: string) =>
  invoke<number>("save_snapshot_real", { name });

/** Restore the VM from a real snapshot (stops the current VM first). */
export const resumeFromSnapshotReal = (name: string) =>
  invoke<void>("resume_from_snapshot_real", { name });

/** Is the host currently holding a (cross-run) VM? */
export const vmIsRunning = () => invoke<boolean>("vm_is_running");

/** VM status for the top-bar badge (v0.3 #4c). */
export interface VmStatus {
  running: boolean;
  since_ms: number | null;
}

export const vmStatus = () => invoke<VmStatus>("vm_status");

/** One-click toolchain download (mirrors the host `DownloadEvent`). */
export type ToolchainDownloadEvent =
  | { kind: "started"; total_bytes: number | null }
  | { kind: "progress"; downloaded: number; total: number | null }
  | { kind: "verifying" }
  | { kind: "extracting" }
  | { kind: "done"; install_path: string }
  | { kind: "failed"; reason: string }
  | { kind: "cancelled" };

export interface ToolchainDownloadStatus {
  in_progress: boolean;
  last_event: ToolchainDownloadEvent | null;
}

/** Start downloading and installing the RISC-V toolchain. */
export const startToolchainDownload = () => invoke<void>("start_toolchain_download");

/** Ask an in-flight toolchain download to stop. */
export const cancelToolchainDownload = () => invoke<void>("cancel_toolchain_download");

/** Whether a download is running, plus the last event seen. */
export const toolchainDownloadStatus = () =>
  invoke<ToolchainDownloadStatus>("toolchain_download_status");

/** Subscribe to `toolchain:download` progress events. */
export const onToolchainDownload = (
  onEvent: (event: ToolchainDownloadEvent) => void,
) => onHostEvent("toolchain:download", (p) => onEvent(p as ToolchainDownloadEvent));

/** QEMU status (v0.3 5b-2): same shape as the toolchain view. */
export interface QemuView {
  found: boolean;
  path: string | null;
  /** "EnvVar" | "KnownPath" | "Path" | "Manual" */
  source: string;
  /** Full search record (where we looked and what happened). */
  diagnostics: string;
}

/** Where QEMU is (and the full search record). */
export const probeQemu = () => invoke<QemuView>("probe_qemu");

/** Same as `probeQemu`; read on mount. */
export const getQemuStatus = () => invoke<QemuView>("get_qemu_status");

/** Point the app at a specific QEMU binary (validated with `--version`). */
export const setQemuPath = (path: string) =>
  invoke<void>("set_qemu_path", { path });

/** Forget the manual QEMU path and go back to auto-discovery. */
export const clearQemuPath = () => invoke<void>("clear_qemu_path");

/** RISC-V GCC toolchain status. */
export interface ToolchainView {
  found: boolean;
  path: string | null;
  /** "EnvVar" | "KnownPath" | "Path" | "Manual" */
  source: string;
  /** Full search record (where we looked and what happened). */
  diagnostics: string;
}

export const probeToolchain = () => invoke<ToolchainView>("probe_toolchain");

export const setToolchainPath = (path: string) =>
  invoke<void>("set_toolchain_path", { path });

export const clearToolchainPath = () => invoke<void>("clear_toolchain_path");

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

export const listSessions = (limit: number) =>
  invoke<SessionMeta[]>("list_sessions", { limit });

export const createSession = (title: string) =>
  invoke<string>("create_session", { title });

export const openSession = (sessionId: string) =>
  invoke<SessionDetail>("open_session", { sessionId });

export const renameSession = (sessionId: string, title: string) =>
  invoke<void>("rename_session", { sessionId, title });

export const deleteSession = (sessionId: string) =>
  invoke<void>("delete_session", { sessionId });

export const clearAllSessions = () => invoke<void>("clear_all_sessions");

export const getCurrentSessionId = () =>
  invoke<string | null>("get_current_session_id");

/** Incremental assistant text from the LLM stream (`agent:stream:delta`). */
export const onAgentStreamDelta = (cb: (text: string) => void) =>
  onHostEvent("agent:stream:delta", (p) =>
    cb((p as { text?: string }).text ?? ""),
  );

/** The LLM stream finished (`agent:stream:done`). */
export const onAgentStreamDone = (cb: () => void) =>
  onHostEvent("agent:stream:done", () => cb());

/** Subscribe to a host event. Returns an unlisten function. */
export const onHostEvent = (
  event: string,
  cb: (payload: unknown) => void,
): (() => void) => {
  const p = listen(event, (e) => cb(e.payload));
  return () => {
    p.then((f) => f()).catch(() => {});
  };
};
