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
  base_url: string;
  model: string;
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

export const setLlmConfig = (apiKey: string, baseUrl: string, model: string) =>
  invoke<void>("set_llm_config", { apiKey, baseUrl, model });

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
