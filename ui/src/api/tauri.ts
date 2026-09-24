// All Tauri `invoke` calls live here, so every backend call is reviewable in
// one place. Never pass secrets anywhere except `setLlmConfig`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { unwrapHostPayload } from "./envelope.ts";
import type {
  AgentOutcomeView,
  AuditEvent,
  AuditStatus,
  FingerprintFieldDiff,
  LlmReadiness,
  LlmStatus,
  LocalProbeResult,
  ProviderPreset,
  PreflightView,
  QemuView,
  RunView,
  SessionDetail,
  SessionMeta,
  SnapshotMeta,
  ToolchainDownloadEvent,
  ToolchainDownloadStatus,
  ToolchainView,
  VmStatus,
} from "./types.ts";

// The shapes moved to `./types` in v0.9 D2b-1, so the Web implementation can name
// them without importing a Tauri module. They are re-exported here because this is
// the module the panels and the store have always read them from.
export type {
  AgentOutcomeView,
  AuditEvent,
  AuditStatus,
  ChainStatus,
  FingerprintFieldDiff,
  HostEnvelope,
  LlmReadiness,
  LlmStatus,
  LocalProbeResult,
  LocalProviderInfo,
  PreflightRow,
  PreflightView,
  ProviderPreset,
  QemuView,
  RunView,
  SessionDetail,
  SessionMessage,
  SessionMeta,
  SnapshotMeta,
  ToolchainDownloadEvent,
  ToolchainDownloadStatus,
  ToolchainView,
  VmStatus,
} from "./types.ts";
export { unwrapHostPayload } from "./envelope.ts";

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

export const listRuns = (limit = 20) =>
  invoke<RunView[]>("list_runs", { limit });

export const getRun = (runId: string) =>
  invoke<RunView | null>("get_run", { runId });

/** UI theme preference (v0.4 #11a): `light` / `dark` / `system`. */
export const getTheme = () => invoke<string>("get_theme");

export const setTheme = (theme: string) => invoke<void>("set_theme", { theme });

/** UI language preference (v0.7 batch 2): `system` / `en` / `zh`. */
export const getLanguage = () => invoke<string>("get_language");

export const setLanguage = (language: string) => invoke<void>("set_language", { language });

/** Turn the audit-failure alert (banner + popup) on or off (v0.8). */
export const setAuditAlert = (enabled: boolean) =>
  invoke<void>("set_audit_alert", { enabled });

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

/**
 * Field-by-field diff of two runs' fingerprints (v0.6 batch 1). The rows arrive
 * in the fingerprint's declaration order and cover every field the two documents
 * carry; the UI renders that order as it is and never re-sorts it.
 */
export const compareRunFingerprints = (runA: string, runB: string) =>
  invoke<FingerprintFieldDiff[]>("compare_run_fingerprints", { runA, runB });

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

export const vmStatus = () => invoke<VmStatus>("vm_status");

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

/** Where QEMU is (and the full search record). */
export const probeQemu = () => invoke<QemuView>("probe_qemu");

/** Same as `probeQemu`; read on mount. */
export const getQemuStatus = () => invoke<QemuView>("get_qemu_status");

/** Point the app at a specific QEMU binary (validated with `--version`). */
export const setQemuPath = (path: string) =>
  invoke<void>("set_qemu_path", { path });

/** Forget the manual QEMU path and go back to auto-discovery. */
export const clearQemuPath = () => invoke<void>("clear_qemu_path");

export const probeToolchain = () => invoke<ToolchainView>("probe_toolchain");

export const setToolchainPath = (path: string) =>
  invoke<void>("set_toolchain_path", { path });

export const clearToolchainPath = () => invoke<void>("clear_toolchain_path");

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
  const p = listen(event, (e) => cb(unwrapHostPayload(e.payload)));
  return () => {
    p.then((f) => f()).catch(() => {});
  };
};
