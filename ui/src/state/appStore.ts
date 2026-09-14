// A tiny store built on useState/useReducer — no state library.
// API key is NOT kept here: it lives only in the Settings form until saved.

import { useCallback, useEffect, useState } from "react";
import * as api from "../api/tauri";

export type Role = "user" | "assistant" | "tool" | "system";

export interface ChatItem {
  id: number;
  role: Role;
  text: string;
  toolName?: string;
  toolArgs?: string;
  toolResult?: string;
  ok?: boolean;
}

export interface AppStore {
  // chat
  messages: ChatItem[];
  busy: boolean;
  send: (input: string) => Promise<void>;
  pushSystem: (text: string) => void;
  // settings
  llmStatus: api.LlmStatus;
  refreshLlmStatus: () => Promise<void>;
  auditStatus: api.AuditStatus | null;
  auditEvents: api.AuditEvent[];
  auditActorFilter: string;
  setAuditActorFilter: (v: string) => void;
  refreshAudit: () => Promise<void>;
  workspaceFiles: string[];
  refreshWorkspace: () => Promise<void>;
  // serial / vm (consumed by the canvas in stage 6c)
  serial: string;
  vmState: string;
  // errors
  lastError: string | null;
}

let nextId = 1;

export function useAppStore(): AppStore {
  const [messages, setMessages] = useState<ChatItem[]>([]);
  const [busy, setBusy] = useState(false);
  const [llmStatus, setLlmStatus] = useState<api.LlmStatus>({
    configured: false,
    provider_id: "deepseek",
    base_url: "",
    model: "",
  });
  const [auditStatus, setAuditStatus] = useState<api.AuditStatus | null>(null);
  const [auditEvents, setAuditEvents] = useState<api.AuditEvent[]>([]);
  const [auditActorFilter, setAuditActorFilter] = useState("");
  const [workspaceFiles, setWorkspaceFiles] = useState<string[]>([]);
  const [serial, setSerial] = useState("");
  const [vmState, setVmState] = useState("idle");
  const [lastError, setLastError] = useState<string | null>(null);

  const push = useCallback((item: Omit<ChatItem, "id">) => {
    setMessages((prev) => [...prev, { ...item, id: nextId++ }]);
  }, []);

  const refreshLlmStatus = useCallback(async () => {
    try {
      setLlmStatus(await api.getLlmConfigStatus());
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const refreshWorkspace = useCallback(async () => {
    try {
      setWorkspaceFiles(await api.getWorkspaceFiles());
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const refreshAudit = useCallback(async () => {
    try {
      const status = await api.getAuditStatus();
      setAuditStatus(status);
      const actor = auditActorFilter.trim() || undefined;
      setAuditEvents(await api.listAuditEvents(50, actor, undefined));
    } catch (e) {
      setLastError(String(e));
    }
  }, [auditActorFilter]);

  // Host events → chat stream / serial / vm state.
  useEffect(() => {
    const offs: Array<() => void> = [];
    offs.push(
      api.onHostEvent("agent:iteration", () => {
        // (a subtle marker; final answer arrives via agent:final)
      }),
      api.onHostEvent("agent:tool_call", (p) => {
        const d = p as { name?: string; arguments?: string };
        push({
          role: "tool",
          text: d.name ?? "tool",
          toolName: d.name ?? "tool",
          toolArgs: d.arguments ?? "",
        });
      }),
      api.onHostEvent("agent:tool_result", (p) => {
        const d = p as { ok?: boolean; result?: string };
        setMessages((prev) => {
          const next = [...prev];
          for (let i = next.length - 1; i >= 0; i--) {
            if (next[i].role === "tool" && next[i].toolResult === undefined) {
              next[i] = {
                ...next[i],
                ok: d.ok,
                toolResult: d.result ?? "",
              };
              break;
            }
          }
          return next;
        });
      }),
      api.onHostEvent("agent:final", (p) => {
        const d = p as api.AgentOutcomeView;
        const text =
          d.kind === "final"
            ? d.content ?? "(no content)"
            : d.kind === "max_iterations"
              ? `Reached iteration limit. ${d.content ?? ""}`
              : `Failed: ${d.reason ?? "unknown"}`;
        push({ role: "assistant", text });
        setBusy(false);
        void refreshAudit();
        void refreshWorkspace();
      }),
      api.onHostEvent("serial:chunk", (p) => {
        const d = p as { chunk?: string };
        setSerial((prev) => prev + (d.chunk ?? ""));
      }),
      api.onHostEvent("vm:state", (p) => {
        const d = p as { state?: string };
        setVmState(d.state ?? "unknown");
      }),
    );
    return () => offs.forEach((f) => f());
  }, [push, refreshAudit, refreshWorkspace]);

  useEffect(() => {
    void refreshLlmStatus();
    void refreshAudit();
    void refreshWorkspace();
  }, [refreshLlmStatus, refreshAudit, refreshWorkspace]);

  const send = useCallback(
    async (input: string) => {
      const text = input.trim();
      if (!text || busy) return;
      push({ role: "user", text });
      setBusy(true);
      setLastError(null);
      try {
        await api.runAgent(text);
      } catch (e) {
        push({ role: "assistant", text: `Error: ${String(e)}` });
        setBusy(false);
      }
    },
    [busy, push],
  );

  const refresh = refreshLlmStatus;

  return {
    messages,
    busy,
    send,
    pushSystem: (text: string) => push({ role: "system", text }),
    llmStatus,
    refreshLlmStatus: refresh,
    auditStatus,
    auditEvents,
    auditActorFilter,
    setAuditActorFilter,
    refreshAudit,
    workspaceFiles,
    refreshWorkspace,
    serial,
    vmState,
    lastError,
  };
}
