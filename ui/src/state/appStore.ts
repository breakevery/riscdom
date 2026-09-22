// A tiny store built on useState/useReducer — no state library.
// API key is NOT kept here: it lives only in the Settings form until saved.

import { useCallback, useEffect, useRef, useSyncExternalStore, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import * as api from "../api/tauri";
import { executableFilters, pickedPath } from "../lib/pathPick";
import { defaultExportPath, MAX_COMPARED_RUNS, toggleRunSelection as nextRunSelection } from "../lib/runView";
import { applyTheme, nextTheme, parseTheme, systemPrefersDark } from "../lib/theme";
import type { ResolvedTheme, Theme } from "../lib/theme";
import {
  getLanguage,
  langAttribute,
  navigatorLanguage,
  parseLanguageChoice,
  resolveLanguage,
  setLanguage as setUiLanguage,
  subscribe as subscribeLanguage,
  t,
} from "../i18n/index.ts";
import type { Language, LanguageChoice } from "../i18n/index.ts";

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
  /**
   * Reload the audit events. `actorOverride` filters by the value the caller just
   * typed, so the list can follow the input without waiting for the state update
   * to land (v0.5 batch 11).
   */
  refreshAudit: (actorOverride?: string) => Promise<void>;
  /**
   * Turn the audit-failure alert (banner + popup) on or off (v0.8). The
   * `audit:failed` event and the log line are not affected.
   */
  setAuditAlert: (enabled: boolean) => Promise<void>;
  runs: api.RunView[];
  refreshRuns: () => Promise<void>;
  /**
   * Export one run's audit interval (v0.5 batch 1) through the native save
   * dialog. Cancelling exports nothing; a failure is reported in
   * `runExportNote` rather than thrown at the panel.
   */
  exportRunAudit: (runId: string) => Promise<void>;
  runExportNote: string | null;
  /**
   * The AI workspace root, as the host reports it (v0.5 batch 2). The audit
   * export's default file name is built under it, so no path is hard-coded here.
   */
  workspaceRoot: string | null;
  /**
   * The runs selected in the audit tab, in click order (v0.5 batch 2). At most
   * `MAX_COMPARED_RUNS`; the side-by-side panel shows exactly two.
   */
  selectedRuns: string[];
  toggleRunSelection: (runId: string) => void;
  clearRunSelection: () => void;
  /**
   * The two runs' fingerprints, field by field (v0.6 batch 1): the host's rows in
   * the host's order, or `null` while there is nothing to show — no pair selected,
   * or the question still in flight. `diffError` carries the host's refusal
   * verbatim (v0.7 batch 2), so the panel builds the sentence at render time and a
   * language change reaches it.
   */
  diffRows: api.FingerprintFieldDiff[] | null;
  diffError: string | null;
  /** Native file pickers (v0.4 batch 2); the manual text entry stays available. */
  pickToolchainPath: () => Promise<void>;
  pickQemuPath: () => Promise<void>;
  /** Environment preflight (v0.4 batch 3). */
  preflight: api.PreflightView | null;
  preflightStep: { step: string; state: string } | null;
  refreshPreflight: () => Promise<void>;
  runPreflight: () => Promise<void>;
  acknowledgePreflight: () => Promise<void>;
  /** Theme (v0.4 #11a). */
  theme: Theme;
  resolvedTheme: ResolvedTheme;
  setTheme: (theme: Theme) => Promise<void>;
  cycleTheme: () => Promise<void>;
  /**
   * Language (v0.7 batches 1-2). `languageChoice` is the stored preference
   * (`system` follows the OS); `language` is what the registry is showing right
   * now, and the store re-renders when it changes.
   */
  languageChoice: LanguageChoice;
  language: Language;
  setLanguage: (choice: LanguageChoice) => Promise<void>;
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
  // snapshots (stage 19c)
  snapshots: api.SnapshotMeta[];
  refreshSnapshots: () => Promise<void>;
  deleteSnapshot: (name: string) => Promise<void>;
  // vm lifecycle + real snapshots (stage 20d)
  vmIsRunning: boolean;
  /** Top-bar VM badge (v0.3 #4c); `vmSeen` stays false until a VM was ever running. */
  vmStatus: { running: boolean; sinceMs: number | null };
  vmSeen: boolean;
  refreshVmState: () => Promise<void>;
  saveSnapshot: (name: string) => Promise<void>;
  resumeSnapshot: (name: string) => Promise<void>;
  // RISC-V toolchain (stage 24b)
  toolchain: api.ToolchainView | null;
  toolchainMissing: boolean;
  refreshToolchain: () => Promise<void>;
  setToolchain: (path: string) => Promise<void>;
  clearToolchain: () => Promise<void>;
  // QEMU (v0.3 5b-2)
  qemu: api.QemuView | null;
  refreshQemu: () => Promise<void>;
  setQemu: (path: string) => Promise<void>;
  clearQemu: () => Promise<void>;
  // one-click toolchain download (v0.3 #3)
  toolchainDownload: {
    in_progress: boolean;
    event: api.ToolchainDownloadEvent | null;
    progress: { downloaded: number; total: number | null } | null;
  };
  startToolchainDownload: () => Promise<void>;
  cancelToolchainDownload: () => Promise<void>;
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
  const [runs, setRuns] = useState<api.RunView[]>([]);
  const [workspaceRoot, setWorkspaceRoot] = useState<string | null>(null);
  const [selectedRuns, setSelectedRuns] = useState<string[]>([]);
  const [diffRows, setDiffRows] = useState<api.FingerprintFieldDiff[] | null>(null);
  const [diffError, setDiffError] = useState<string | null>(null);
  const [preflight, setPreflight] = useState<api.PreflightView | null>(null);
  const [preflightStep, setPreflightStep] = useState<{
    step: string;
    state: string;
  } | null>(null);
  const [theme, setThemeChoice] = useState<Theme>("system");
  const [resolvedTheme, setResolvedTheme] = useState<ResolvedTheme>("dark");
  const [systemDark, setSystemDark] = useState<boolean>(() => systemPrefersDark(window));
  // Language (v0.7 batches 1-2). `languageChoice` is the preference, stored in
  // settings.json like the theme; `language` is what the registry shows right
  // now. The store subscribes to the registry (useSyncExternalStore), so a
  // language change re-renders the tree instead of leaving already-rendered text
  // in the old language.
  const [languageChoice, setLanguageChoice] = useState<LanguageChoice>("system");
  const language = useSyncExternalStore(subscribeLanguage, getLanguage);
  const languageChoiceRef = useRef<LanguageChoice>(languageChoice);
  const [workspaceFiles, setWorkspaceFiles] = useState<string[]>([]);
  const [serial, setSerial] = useState("");
  const [vmState, setVmState] = useState("idle");
  // Live assistant text from `agent:stream:*`; replaced by the final content.
  const [streaming, setStreaming] = useState("");
  const [streamingActive, setStreamingActive] = useState(false);
  const [sessions, setSessions] = useState<api.SessionMeta[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);
  const [snapshots, setSnapshots] = useState<api.SnapshotMeta[]>([]);
  const [vmIsRunning, setVmIsRunning] = useState(false);
  const [vmStatus, setVmStatus] = useState<{ running: boolean; sinceMs: number | null }>({
    running: false,
    sinceMs: null,
  });
  const [vmSeen, setVmSeen] = useState(false);
  const [toolchain, setToolchain] = useState<api.ToolchainView | null>(null);
  const [qemu, setQemu] = useState<api.QemuView | null>(null);
  const [toolchainDownload, setToolchainDownload] = useState<{
    in_progress: boolean;
    event: api.ToolchainDownloadEvent | null;
    progress: { downloaded: number; total: number | null } | null;
  }>({ in_progress: false, event: null, progress: null });
  const [lastError, setLastError] = useState<string | null>(null);
  // The last run-export outcome (v0.5 batch 1): shown in the audit tab.
  const [runExportNote, setRunExportNote] = useState<string | null>(null);

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
      setWorkspaceRoot(await api.getWorkspaceRoot());
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const refreshAudit = useCallback(
    async (actorOverride?: string) => {
      try {
        const status = await api.getAuditStatus();
        setAuditStatus(status);
        const actor =
          (actorOverride ?? auditActorFilter).trim() || undefined;
        setAuditEvents(await api.listAuditEvents(50, actor, undefined));
      } catch (e) {
        setLastError(String(e));
      }
    },
    [auditActorFilter],
  );

  // The audit-failure alert (v0.8): the setting lives in settings.json like the
  // theme and the language. Persisting it and refreshing the status is all this
  // does — the event and the log line are sent whatever the choice.
  const setAuditAlert = useCallback(
    async (enabled: boolean) => {
      try {
        await api.setAuditAlert(enabled);
        await refreshAudit();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [refreshAudit],
  );

  // Runs come from the host's derived index (v0.4 1d). Nothing here mutates a
  // run: the panel is a window onto what the chain already says.
  const refreshRuns = useCallback(async () => {
    try {
      setRuns(await api.listRuns(20));
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  // Export one run's audit interval (v0.5 batches 1-2). The default lands in the
  // workspace, named after the run; the host still refuses a path outside it, so
  // the note shows whatever the host said instead of pretending the export worked.
  const exportRunAudit = useCallback(
    async (runId: string) => {
      try {
        const target = await save({
          defaultPath: defaultExportPath(workspaceRoot, runId),
          filters: [{ name: "JSONL", extensions: ["jsonl"] }],
        });
        if (!target) return; // a cancelled dialog exports nothing
        const lines = await api.exportRunAudit(runId, target);
        setRunExportNote(t("audit.export_done", { runId, lines, target }));
      } catch (e) {
        setRunExportNote(t("audit.export_failed", { runId, reason: String(e) }));
      }
    },
    [workspaceRoot],
  );

  // Two-run selection for the side-by-side panel (v0.5 batch 2). The rule lives in
  // `runView.ts` so the probe can check it without a React harness.
  const toggleRunSelection = useCallback((runId: string) => {
    setSelectedRuns((prev) => nextRunSelection(prev, runId));
  }, []);

  const clearRunSelection = useCallback(() => setSelectedRuns([]), []);

  // The field-level diff of the two selected runs (v0.6 batch 1). It is the host's
  // answer — both fingerprint documents are read off the chain there — and the UI
  // renders the rows in exactly the order they arrive: no re-sorting, no diffing
  // JSON here. Changing the pair, or clearing it, supersedes the answer in flight.
  // `diffError` keeps the host's refusal **verbatim**: the sentence is built at
  // render time, so a language change reaches it too (v0.7 batch 2).
  useEffect(() => {
    if (selectedRuns.length !== MAX_COMPARED_RUNS) {
      setDiffRows(null);
      setDiffError(null);
      return;
    }
    let cancelled = false;
    const [runA, runB] = selectedRuns;
    api
      .compareRunFingerprints(runA, runB)
      .then((rows) => {
        if (cancelled) return;
        setDiffRows(rows);
        setDiffError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setDiffRows(null);
        setDiffError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedRuns]);

  // Environment preflight (v0.4 batch 3): the run itself is asynchronous, so the
  // UI follows `preflight:progress` and re-reads the cached result at the end.
  const refreshPreflight = useCallback(async () => {
    try {
      setPreflight(await api.preflightStatus());
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const runPreflight = useCallback(async () => {
    try {
      setPreflightStep(null);
      await api.runPreflight();
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const acknowledgePreflight = useCallback(async () => {
    try {
      setPreflight(await api.acknowledgePreflight());
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  // Theme (v0.4 #11a): the choice lives in settings.json, the applied value follows
  // the OS while the choice is "system".
  useEffect(() => {
    if (!window.matchMedia) return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    setResolvedTheme(applyTheme(theme, document.documentElement, systemDark));
  }, [theme, systemDark]);

  // Language (v0.7 batches 1-2): the preference lives in settings.json (like the
  // theme), the registry shows the resolved language and `lang` follows it. The
  // registry write comes first, so the re-render it triggers already reads the new
  // language — no reload, and never a frame in the previous language.
  const applyLanguageChoice = useCallback((choice: LanguageChoice, tag: unknown) => {
    const resolved = resolveLanguage(choice, tag);
    setUiLanguage(resolved);
    document.documentElement.lang = langAttribute(resolved);
  }, []);

  useEffect(() => {
    languageChoiceRef.current = languageChoice;
  }, [languageChoice]);

  // While the choice is `system`, follow the OS language the way the theme follows
  // the colour scheme.
  useEffect(() => {
    if (!window.addEventListener) return;
    const onLanguageChange = () => {
      if (languageChoiceRef.current === "system") {
        applyLanguageChoice("system", navigatorLanguage());
      }
    };
    window.addEventListener("languagechange", onLanguageChange);
    return () => window.removeEventListener("languagechange", onLanguageChange);
  }, [applyLanguageChoice]);

  useEffect(() => {
    void (async () => {
      try {
        const stored = parseLanguageChoice(await api.getLanguage());
        setLanguageChoice(stored);
        applyLanguageChoice(stored, navigatorLanguage());
      } catch (e) {
        setLastError(String(e));
      }
    })();
  }, [applyLanguageChoice]);

  useEffect(() => {
    void (async () => {
      try {
        setThemeChoice(parseTheme(await api.getTheme()));
      } catch (e) {
        setLastError(String(e));
      }
    })();
  }, []);

  const setTheme = useCallback(async (next: Theme) => {
    setThemeChoice(next);
    try {
      await api.setTheme(next);
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const cycleTheme = useCallback(async () => {
    await setTheme(nextTheme(theme));
  }, [setTheme, theme]);

  // The settings page's language choice (v0.7 batch 2): apply it first, then
  // store it, then persist it, so the visible language never waits on disk.
  const setLanguage = useCallback(
    async (next: LanguageChoice) => {
      applyLanguageChoice(next, navigatorLanguage());
      setLanguageChoice(next);
      try {
        await api.setLanguage(next);
      } catch (e) {
        setLastError(String(e));
      }
    },
    [applyLanguageChoice],
  );

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
      const id = await api.createSession(t("chat.session_default_title"));
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

  const refreshSnapshots = useCallback(async () => {
    try {
      setSnapshots(await api.listSnapshots());
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const deleteSnapshot = useCallback(
    async (name: string) => {
      try {
        await api.deleteSnapshot(name);
        await refreshSnapshots();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [refreshSnapshots],
  );

  useEffect(() => {
    void refreshSnapshots();
  }, [refreshSnapshots]);

  const refreshVmState = useCallback(async () => {
    try {
      setVmIsRunning(await api.vmIsRunning());
      const status = await api.vmStatus();
      setVmStatus({ running: status.running, sinceMs: status.since_ms });
      if (status.running) setVmSeen(true);
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  // Save the VM's current state as a real snapshot (`<name>.mig`).
  const saveSnapshot = useCallback(
    async (name: string) => {
      try {
        await api.saveSnapshotReal(name);
        await refreshSnapshots();
        await refreshVmState();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [refreshSnapshots, refreshVmState],
  );

  // Restore the VM from a real snapshot (the current VM is stopped first).
  const resumeSnapshot = useCallback(
    async (name: string) => {
      try {
        await api.resumeFromSnapshotReal(name);
        await refreshVmState();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [refreshVmState],
  );

  const refreshToolchain = useCallback(async () => {
    try {
      setToolchain(await api.probeToolchain());
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const setToolchainPath = useCallback(
    async (path: string) => {
      try {
        await api.setToolchainPath(path);
        await refreshToolchain();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [refreshToolchain],
  );

  const clearToolchain = useCallback(async () => {
    try {
      await api.clearToolchainPath();
      await refreshToolchain();
    } catch (e) {
      setLastError(String(e));
    }
  }, [refreshToolchain]);

  useEffect(() => {
    void refreshToolchain();
  }, [refreshToolchain]);

  const refreshQemu = useCallback(async () => {
    try {
      setQemu(await api.getQemuStatus());
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const setQemuPath = useCallback(
    async (path: string) => {
      try {
        await api.setQemuPath(path);
        await refreshQemu();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [refreshQemu],
  );

  // Native pickers (v0.4 batch 2). A cancelled dialog changes nothing; the manual
  // text entry stays next to these for the cases a picker cannot serve.
  const pickToolchainPath = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: t("toolchain.pick_gcc_title"),
        filters: executableFilters("RISC-V GCC", navigator.userAgent),
      });
      const path = pickedPath(selected);
      if (path) await setToolchainPath(path);
    } catch (e) {
      setLastError(String(e));
    }
  }, [setToolchainPath]);

  const pickQemuPath = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: t("toolchain.pick_qemu_title"),
        filters: executableFilters("QEMU", navigator.userAgent),
      });
      const path = pickedPath(selected);
      if (path) await setQemuPath(path);
    } catch (e) {
      setLastError(String(e));
    }
  }, [setQemuPath]);

  const clearQemu = useCallback(async () => {
    try {
      await api.clearQemuPath();
      await refreshQemu();
    } catch (e) {
      setLastError(String(e));
    }
  }, [refreshQemu]);

  useEffect(() => {
    void refreshQemu();
  }, [refreshQemu]);

  // Sync the download state once (a download may already be running).
  useEffect(() => {
    void (async () => {
      try {
        const status = await api.toolchainDownloadStatus();
        setToolchainDownload((prev) => ({
          ...prev,
          in_progress: status.in_progress,
          event: status.last_event ?? prev.event,
        }));
      } catch {
        /* ignore: the panel still works via events */
      }
    })();
  }, []);

  const startToolchainDownload = useCallback(async () => {
    try {
      await api.startToolchainDownload();
      setToolchainDownload({
        in_progress: true,
        event: { kind: "started", total_bytes: null },
        progress: null,
      });
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const cancelToolchainDownload = useCallback(async () => {
    try {
      await api.cancelToolchainDownload();
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

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
        void refreshRuns();
        void refreshWorkspace();
        void refreshSessions();
        // The VM (if the run started one) now lives in the host slot.
        void refreshVmState();
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
        const d = p as { state?: string; running?: boolean; since_ms?: number | null };
        setVmState(d.state ?? "unknown");
        if (typeof d.running === "boolean") {
          setVmStatus({ running: d.running, sinceMs: d.since_ms ?? null });
          if (d.running) setVmSeen(true);
        }
        void refreshVmState();
      }),
      api.onHostEvent("preflight:progress", (p) => {
        const d = p as { step?: string; state?: string };
        if (!d.step || !d.state) return;
        setPreflightStep({ step: d.step, state: d.state });
        if (d.step === "done") void refreshPreflight();
      }),
      api.onToolchainDownload((event) => {
        setToolchainDownload((prev) => ({
          in_progress:
            event.kind !== "done" &&
            event.kind !== "failed" &&
            event.kind !== "cancelled",
          event,
          progress:
            event.kind === "progress"
              ? { downloaded: event.downloaded, total: event.total }
              : event.kind === "started"
                ? null
                : prev.progress,
        }));
        // A finished download becomes the active toolchain.
        if (event.kind === "done") void refreshToolchain();
      }),
    );
    return () => offs.forEach((f) => f());
  }, [push, refreshAudit, refreshRuns, refreshPreflight, refreshWorkspace]);

  useEffect(() => {
    void refreshLlmStatus();
    void refreshAudit();
    void refreshRuns();
    void refreshPreflight();
    void refreshWorkspace();
  }, [refreshLlmStatus, refreshAudit, refreshRuns, refreshPreflight, refreshWorkspace]);

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
    setAuditAlert,
    auditEvents,
    auditActorFilter,
    setAuditActorFilter,
    refreshAudit,
    runs,
    refreshRuns,
    exportRunAudit,
    runExportNote,
    workspaceRoot,
    selectedRuns,
    toggleRunSelection,
    clearRunSelection,
    diffRows,
    diffError,
    workspaceFiles,
    refreshWorkspace,
    serial,
    vmState,
    streaming,
    streamingActive,
    sessions,
    currentSessionId,
    snapshots,
    refreshSnapshots,
    deleteSnapshot,
    vmIsRunning,
    vmStatus,
    vmSeen,
    refreshVmState,
    saveSnapshot,
    resumeSnapshot,
    toolchain,
    toolchainMissing: toolchain !== null && !toolchain.found,
    refreshToolchain,
    setToolchain: setToolchainPath,
    pickToolchainPath,
    pickQemuPath,
    preflight,
    preflightStep,
    refreshPreflight,
    runPreflight,
    acknowledgePreflight,
    theme,
    resolvedTheme,
    setTheme,
    cycleTheme,
    languageChoice,
    language,
    setLanguage,
    clearToolchain,
    qemu,
    refreshQemu,
    setQemu: setQemuPath,
    clearQemu,
    toolchainDownload,
    startToolchainDownload,
    cancelToolchainDownload,
    refreshSessions,
    openSession,
    newSession,
    renameSession,
    deleteSession,
    lastError,
  };
}
