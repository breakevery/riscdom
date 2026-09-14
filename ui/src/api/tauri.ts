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

export const getSerialBuffer = () => invoke<string>("get_serial_buffer");

export const exportSerialLog = (path: string) =>
  invoke<number>("export_serial_log", { path });

export const exportAuditJsonl = (path: string) =>
  invoke<number>("export_audit_jsonl", { path });

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
