/**
 * The one API surface the UI imports (v0.9 D2b-1).
 *
 * Two implementations, one interface:
 *
 * - `api/tauri.ts` — the desktop shell, over the Tauri IPC (`invoke` / `listen`);
 * - `api/http.ts` — the browser, over the control plane's HTTP endpoints.
 *
 * Which one is live is decided **at runtime, once**, from the presence of Tauri's
 * own global: Tauri 2 injects `window.__TAURI_INTERNALS__` before any application
 * code runs, and `@tauri-apps/api` calls `invoke` through exactly that object
 * (`@tauri-apps/api/core.js`). A browser has no such global, so nothing needs
 * configuring and **one built `dist/` serves both front ends** — the desktop shell
 * loads it from disk, and the server serves the same files with `--web-root`.
 *
 * The list below is the interface, spelled out one name at a time. It is longer
 * than `export *` would be and buys the thing that matters: `impl` is typed as one
 * implementation, so a name that exists in one and not the other — or a signature
 * that drifts — fails to compile here instead of at a call site in a panel.
 */

import * as http from "./http.ts";
import * as tauri from "./tauri.ts";

/** The shapes. They belong to neither transport: see `api/types.ts`. */
export * from "./types.ts";

/** The one rule both implementations share, for the same reason. */
export { unwrapHostPayload } from "./envelope.ts";

/**
 * Is this the Tauri webview?
 *
 * `window.__TAURI_INTERNALS__` is Tauri 2's own marker; nothing else defines it,
 * and in a browser this is `false` (including during a server-side test, where
 * there is no `window` at all).
 */
export function isTauriRuntime(): boolean {
  if (typeof window === "undefined") return false;
  return (
    typeof (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !==
    "undefined"
  );
}

/**
 * The names both implementations have to carry.
 *
 * One implementation's shape minus the Web client's own three helpers
 * (`setApiBase` / `setToken` / `currentToken`), which the desktop has no use for:
 * assigning the Tauri module to this type is what makes a name that only exists in
 * one of the two — or a signature that drifted — a compile error right here.
 */
type SharedApi = Omit<
  typeof http,
  | "setApiBase"
  | "setToken"
  | "currentToken"
  | "clearToken"
  | "verifyToken"
  | "getHealth"
  | "getStatus"
  | "onGap"
>;

/**
 * Whichever implementation this runtime has.
 */
const impl: SharedApi = isTauriRuntime() ? tauri : http;

/**
 * Token plumbing, and it is the Web client's alone.
 *
 * On the desktop these do nothing that matters — the shell's `invoke` is
 * authenticated by being in the same process.
 *
 * They are named **outside** the shared list below rather than added to it: that
 * list is the check that the two *transports* agree with each other, and none of
 * these is a transport call. `getHealth` / `getStatus` are the local endpoints the
 * Web client asks about the node; the desktop answers those questions through the
 * store instead.
 */
export {
  clearToken,
  currentToken,
  getHealth,
  getStatus,
  onGap,
  setApiBase,
  setToken,
  verifyToken,
} from "./http.ts";
export type { TokenCheck } from "./http.ts";

// ----- Reads -----------------------------------------------------------------

export const getAuditStatus = impl.getAuditStatus;

export const listAuditEvents = impl.listAuditEvents;

export const listRuns = impl.listRuns;

export const getRun = impl.getRun;

export const compareRunFingerprints = impl.compareRunFingerprints;

export const getProviderPresets = impl.getProviderPresets;

export const getLlmConfigStatus = impl.getLlmConfigStatus;

export const getLlmReadiness = impl.getLlmReadiness;

export const probeLocalLlm = impl.probeLocalLlm;

export const hasStoredKey = impl.hasStoredKey;

export const listSessions = impl.listSessions;

export const getCurrentSessionId = impl.getCurrentSessionId;

export const listSnapshots = impl.listSnapshots;

export const vmIsRunning = impl.vmIsRunning;

export const vmStatus = impl.vmStatus;

export const probeToolchain = impl.probeToolchain;

export const toolchainDownloadStatus = impl.toolchainDownloadStatus;

export const probeQemu = impl.probeQemu;

export const getQemuStatus = impl.getQemuStatus;

export const preflightStatus = impl.preflightStatus;

export const getTheme = impl.getTheme;

export const getLanguage = impl.getLanguage;

export const getWorkspaceRoot = impl.getWorkspaceRoot;

export const getWorkspaceFiles = impl.getWorkspaceFiles;

export const readWorkspaceFile = impl.readWorkspaceFile;

export const getSerialBuffer = impl.getSerialBuffer;

// ----- The node's own inventory (shared names, v0.9 D2b-4b) -------------------
// These four are the desktop's commands too (`list_executors`, `list_sandboxes`,
// `current_sandbox`, `sandbox_candidates`), so they are *not* Web-only: both
// implementations carry them and the shared list above stays the check that they agree.

export const listExecutors = impl.listExecutors;

export const listSandboxes = impl.listSandboxes;

export const currentSandbox = impl.currentSandbox;

export const sandboxCandidates = impl.sandboxCandidates;

// ----- Controls (implemented on the desktop; the Web client says so) ---------

export const setTheme = impl.setTheme;

export const setLanguage = impl.setLanguage;

export const setAuditAlert = impl.setAuditAlert;

export const runPreflight = impl.runPreflight;

export const acknowledgePreflight = impl.acknowledgePreflight;

export const setLlmConfig = impl.setLlmConfig;

export const loadStoredKey = impl.loadStoredKey;

export const clearLlmConfig = impl.clearLlmConfig;

export const runAgent = impl.runAgent;

export const exportSerialLog = impl.exportSerialLog;

export const exportAuditJsonl = impl.exportAuditJsonl;

export const exportRunAudit = impl.exportRunAudit;

export const deleteSnapshot = impl.deleteSnapshot;

export const saveSnapshotReal = impl.saveSnapshotReal;

export const resumeFromSnapshotReal = impl.resumeFromSnapshotReal;

export const startToolchainDownload = impl.startToolchainDownload;

export const cancelToolchainDownload = impl.cancelToolchainDownload;

export const setQemuPath = impl.setQemuPath;

export const clearQemuPath = impl.clearQemuPath;

export const setToolchainPath = impl.setToolchainPath;

export const clearToolchainPath = impl.clearToolchainPath;

export const createSession = impl.createSession;

export const openSession = impl.openSession;

export const renameSession = impl.renameSession;

export const deleteSession = impl.deleteSession;

export const clearAllSessions = impl.clearAllSessions;

// ----- Host events -----------------------------------------------------------

export const onHostEvent = impl.onHostEvent;

export const onToolchainDownload = impl.onToolchainDownload;

export const onAgentStreamDelta = impl.onAgentStreamDelta;

export const onAgentStreamDone = impl.onAgentStreamDone;
