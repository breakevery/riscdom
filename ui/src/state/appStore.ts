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
  // live LLM stream (stage 16c)
  streaming: string;
  streamingActive: boolean;
  // sessions (stage 17c)
  sessions: api.SessionMeta[];
  currentSessionId: string | null;
  refreshSessions: () => Promise<void>;
  openSession: (id: string) => Promise<void>;
  newSession: () => Promise<void>;
  renameSession: (id: string, title: string) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  // errors
  lastError: string | null;
}

let nextId = 1;

/** Map a persisted message back into a chat item (system rows are dropped). */
function historyItem(row: api.SessionMessage, id: number): ChatItem | null {
  if (row.role === "system") return null;
  if (row.role === "tool") {
    return {
      id,
      role: "tool",
      text: "tool",
      toolName: "tool",
      toolArgs: "",
      toolResult: row.content,
      ok: true,
    };
  }
  if (row.role === "assistant" && row.tool_call_json) {
    let name = "tool";
    let args = "";
    try {
      const calls = JSON.parse(row.tool_call_json) as Array<{
        function?: { name?: string; arguments?: string };
      }>;
      name = calls[0]?.function?.name ?? name;
      args = calls[0]?.function?.arguments ?? args;
    } catch {
      /* ignore malformed history */
    }
    return { id, role: "tool", text: name, toolName: name, toolArgs: args };
  }
  return {
    id,
    role: row.role === "assistant" ? "assistant" : "user",
    text: row.content,
  };
}

/** Short relative time for the session list. */
export function relTime(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}

export function useAppStore(): AppStore {
  const [messages, setMessages] = useState<ChatItem[]>([]);
  const [busy, setBusy] = useState(false);
  const [llmStatus, setLlmStatus] = useState<api.LlmStatus>({
    configured: false,
    provider_id: "deepseek",
    base_url: "",
    model: "",
    persisted: false,
  });
  const [auditStatus, setAuditStatus] = useState<api.AuditStatus | null>(null);
  const [auditEvents, setAuditEvents] = useState<api.AuditEvent[]>([]);
  const [auditActorFilter, setAuditActorFilter] = useState("");
  const [workspaceFiles, setWorkspaceFiles] = useState<string[]>([]);
  const [serial, setSerial] = useState("");
  const [vmState, setVmState] = useState("idle");
  // Live assistant text from `agent:stream:*`; replaced by the final content.
  const [streaming, setStreaming] = useState("");
  const [streamingActive, setStreamingActive] = useState(false);
  const [sessions, setSessions] = useState<api.SessionMeta[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);
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

  // ---- sessions (declared before the event subscription that uses them) ----

  const refreshSessions = useCallback(async () => {
    try {
      setSessions(await api.listSessions(50));
      setCurrentSessionId(await api.getCurrentSessionId());
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const restoreMessages = useCallback((rows: api.SessionMessage[]) => {
    const items: ChatItem[] = [];
    for (const row of rows) {
      const item = historyItem(row, nextId++);
      if (item) items.push(item);
    }
    setMessages(items);
  }, []);

  const openSession = useCallback(
    async (id: string) => {
      try {
        const detail = await api.openSession(id);
        restoreMessages(detail.messages);
        setCurrentSessionId(detail.meta.id);
        setStreaming("");
        setStreamingActive(false);
        await refreshSessions();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [restoreMessages, refreshSessions],
  );

  const newSession = useCallback(async () => {
    try {
      const id = await api.createSession("新会话");
      setMessages([]);
      setStreaming("");
      setStreamingActive(false);
      setCurrentSessionId(id);
      await refreshSessions();
    } catch (e) {
      setLastError(String(e));
    }
  }, [refreshSessions]);

  const renameSession = useCallback(
    async (id: string, title: string) => {
      try {
        await api.renameSession(id, title);
        await refreshSessions();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [refreshSessions],
  );

  const deleteSession = useCallback(
    async (id: string) => {
      try {
        await api.deleteSession(id);
        if (currentSessionId === id) {
          setMessages([]);
          setCurrentSessionId(null);
        }
        await refreshSessions();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [currentSessionId, refreshSessions],
  );

  useEffect(() => {
    void refreshSessions();
  }, [refreshSessions]);

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
        // The final content supersedes anything streamed incrementally.
        setStreaming("");
        setStreamingActive(false);
        setBusy(false);
        void refreshAudit();
        void refreshWorkspace();
        void refreshSessions();
      }),
      api.onAgentStreamDelta((text) => {
        if (!text) return;
        setStreaming((prev) => prev + text);
        setStreamingActive(true);
      }),
      api.onAgentStreamDone(() => {
        setStreamingActive(false);
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
    streaming,
    streamingActive,
    sessions,
    currentSessionId,
    refreshSessions,
    openSession,
    newSession,
    renameSession,
    deleteSession,
    lastError,
  };
}
