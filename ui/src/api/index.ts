/**
 * The one API surface the UI imports (v0.9 D2b-1; the mode is v0.9.9 "out").
 *
 * Two implementations, one interface:
 *
 * - `api/tauri.ts` — the desktop shell, over the Tauri IPC (`invoke` / `listen`);
 * - `api/http.ts` — the browser, over the control plane's HTTP endpoints.
 *
 * Which one is live is decided **at runtime** from the presence of Tauri's own
 * global: Tauri 2 injects `window.__TAURI_INTERNALS__` before any application code
 * runs, and `@tauri-apps/api` calls `invoke` through exactly that object
 * (`@tauri-apps/api/core.js`). A browser has no such global, so nothing needs
 * configuring and **one built `dist/` serves both front ends** — the desktop shell
 * loads it from disk, and the server serves the same files with `--web-root`.
 *
 * v0.9.9 adds a second question on top of that one, and the two are not the same:
 *
 * - **where am I running** — `isTauriRuntime()`, decided once and never changing;
 * - **which host am I talking to** — the *mode*: the embedded host in this process
 *   (`local`), or an in-network server this desktop connected to (`remote`).
 *
 * A desktop in `remote` mode talks over HTTP exactly like the browser does, to
 * another machine's node. That is why this module holds `current` in a variable
 * rather than in the `const` it began as: the mode is settled at startup, before
 * the first panel renders, but it is settled by *code* (a settings read), not by
 * the environment. **The exports below are one-line forwarders** so that every
 * consumer keeps importing the adapter; the alternative — swapping the module —
 * would need a reload and would still not be enough for `isRemote()`.
 *
 * The list below is the interface, spelled out one name at a time. It is longer
 * than `export *` would be and buys the thing that matters: `SharedApi` is typed as
 * one implementation, so a name that exists in one and not the other — or a
 * signature that drifts — fails to compile here instead of at a call site in a
 * panel. The forwarders keep that: each one is typed as the *shared* signature, so
 * a forwarder for a name one implementation lacks does not compile either.
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
 *
 * **It is not the same question as [`isLocalHost`].** This answers "what am I
 * running inside"; that answers "whose node am I looking at". A desktop in remote
 * mode is a Tauri runtime that is *not* the local host, which is why the
 * components that used to ask this one now ask the other.
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
 * One implementation's shape minus the Web client's own helpers
 * (`setApiBase` / `getApiBase` / `setToken` / `currentToken` / `clearToken` /
 * `verifyToken` / `getHealth` / `getStatus` / `onGap`), which are the *control
 * channel* rather than the data plane: a token to present, an address to present it
 * to, and "is the node answering at all". The desktop has none of them — its host is
 * in the same process — so they are named below, outside this list.
 *
 * Assigning the Tauri module to this type is what makes a name that only exists in
 * one of the two — or a signature that drifted — a compile error right here.
 */
type SharedApi = Omit<
  typeof http,
  | "setApiBase"
  | "getApiBase"
  | "setToken"
  | "currentToken"
  | "clearToken"
  | "verifyToken"
  | "getHealth"
  | "getStatus"
  | "onGap"
>;

/** Which host this window talks to. See the note at the top of this file. */
export type Mode = "local" | "remote";

/**
 * Whichever implementation the mode has.
 *
 * It is a variable, not a `const`, because the mode is read from settings during
 * startup — see [`setImpl`]. Nothing else may reassign it.
 */
let current: SharedApi = isTauriRuntime() ? tauri : http;

/** The mode in force. `local` until [`setImpl`] says otherwise. */
let mode: Mode = "local";

/**
 * Talk to the embedded host (`local`), or to an in-network server (`remote`).
 *
 * Called **once, during startup**, before the shell mounts: the mode decides which
 * transport every call below forwards to, and a panel that had already read from
 * one host would be showing that host's data under the other one's name.
 *
 * `remote` also installs the address, because the two are one decision: the HTTP
 * implementation dials whatever `setApiBase` was told, and a remote mode with no
 * address would dial the page's own origin.
 *
 * A browser may call this; it changes nothing there (`remote` and `local` are both
 * the HTTP implementation, and a browser has no embedded host to return to).
 */
export function setImpl(next: Mode, url = ""): void {
  if (next === "remote") {
    http.setApiBase(url);
    current = http;
    mode = "remote";
    return;
  }
  http.setApiBase("");
  current = isTauriRuntime() ? tauri : http;
  mode = "local";
}

/** Is this window showing an in-network server's node rather than its own? */
export function isRemote(): boolean {
  return mode === "remote";
}

/**
 * Is this window talking to the node in its own process?
 *
 * The gate's question, and the settings page's, and the event stream's: the Web
 * client and a desktop in remote mode are both "not the local host" — they read
 * over HTTP, they can lose frames, and they cannot rewire the node they are
 * looking at.
 */
export function isLocalHost(): boolean {
  return isTauriRuntime() && mode !== "remote";
}

/**
 * Token plumbing, and it is the control channel rather than the data plane.
 *
 * On the desktop's **local** mode these do nothing that matters — the shell's
 * `invoke` is authenticated by being in the same process. In **remote** mode they
 * are exactly what the browser uses them for: `setApiBase` says where, `setToken`
 * says who, and `verifyToken` asks whether the pair is accepted.
 *
 * They are named **outside** the shared list below rather than added to it: that
 * list is the check that the two *transports* agree with each other, and none of
 * these is a transport call.
 */
export {
  clearToken,
  currentToken,
  getApiBase,
  getHealth,
  getStatus,
  onGap,
  setApiBase,
  setToken,
  verifyToken,
} from "./http.ts";
export type { TokenCheck } from "./http.ts";

// ----- The node's own wiring, which is local in every mode -------------------
//
// These eight are the exception to the forwarding above, and deliberately so:
// **rewiring a node is a local act**. A desktop that is looking at another
// machine's node must still be able to serve its own board, show its own token and
// leave the remote one — and in remote mode the forwarding would send them to the
// remote server, which has no such command (its copies reject with a sentence).
// So they go to the shell on the desktop, and to the Web client's refusals in a
// browser, where the network face is not offered at all.
//
// `probe-ui-remote.mjs` holds them to this: the mode must not change what these
// eight address.

export const getNetwork: SharedApi["getNetwork"] = (...args) =>
  isTauriRuntime() ? tauri.getNetwork(...args) : http.getNetwork(...args);

export const setNetwork: SharedApi["setNetwork"] = (...args) =>
  isTauriRuntime() ? tauri.setNetwork(...args) : http.setNetwork(...args);

export const readLanToken: SharedApi["readLanToken"] = (...args) =>
  isTauriRuntime() ? tauri.readLanToken(...args) : http.readLanToken(...args);

export const lanStatus: SharedApi["lanStatus"] = (...args) =>
  isTauriRuntime() ? tauri.lanStatus(...args) : http.lanStatus(...args);

export const saveRemoteToken: SharedApi["saveRemoteToken"] = (...args) =>
  isTauriRuntime() ? tauri.saveRemoteToken(...args) : http.saveRemoteToken(...args);

export const readRemoteToken: SharedApi["readRemoteToken"] = (...args) =>
  isTauriRuntime() ? tauri.readRemoteToken(...args) : http.readRemoteToken(...args);

export const clearRemoteToken: SharedApi["clearRemoteToken"] = (...args) =>
  isTauriRuntime() ? tauri.clearRemoteToken(...args) : http.clearRemoteToken(...args);

export const restartApp: SharedApi["restartApp"] = (...args) =>
  isTauriRuntime() ? tauri.restartApp(...args) : http.restartApp(...args);

// ----- Reads -----------------------------------------------------------------

export const getAuditStatus: SharedApi["getAuditStatus"] = (...args) =>
  current.getAuditStatus(...args);

export const listAuditEvents: SharedApi["listAuditEvents"] = (...args) =>
  current.listAuditEvents(...args);

export const listRuns: SharedApi["listRuns"] = (...args) => current.listRuns(...args);

export const getRun: SharedApi["getRun"] = (...args) => current.getRun(...args);

export const compareRunFingerprints: SharedApi["compareRunFingerprints"] = (...args) =>
  current.compareRunFingerprints(...args);

export const getProviderPresets: SharedApi["getProviderPresets"] = (...args) =>
  current.getProviderPresets(...args);

export const getLlmConfigStatus: SharedApi["getLlmConfigStatus"] = (...args) =>
  current.getLlmConfigStatus(...args);

export const getLlmReadiness: SharedApi["getLlmReadiness"] = (...args) =>
  current.getLlmReadiness(...args);

export const probeLocalLlm: SharedApi["probeLocalLlm"] = (...args) =>
  current.probeLocalLlm(...args);

export const hasStoredKey: SharedApi["hasStoredKey"] = (...args) =>
  current.hasStoredKey(...args);

export const listSessions: SharedApi["listSessions"] = (...args) =>
  current.listSessions(...args);

export const getCurrentSessionId: SharedApi["getCurrentSessionId"] = (...args) =>
  current.getCurrentSessionId(...args);

export const listSnapshots: SharedApi["listSnapshots"] = (...args) =>
  current.listSnapshots(...args);

export const vmIsRunning: SharedApi["vmIsRunning"] = (...args) =>
  current.vmIsRunning(...args);

export const vmStatus: SharedApi["vmStatus"] = (...args) => current.vmStatus(...args);

export const probeToolchain: SharedApi["probeToolchain"] = (...args) =>
  current.probeToolchain(...args);

export const toolchainDownloadStatus: SharedApi["toolchainDownloadStatus"] = (...args) =>
  current.toolchainDownloadStatus(...args);

export const probeQemu: SharedApi["probeQemu"] = (...args) => current.probeQemu(...args);

export const getQemuStatus: SharedApi["getQemuStatus"] = (...args) =>
  current.getQemuStatus(...args);

export const preflightStatus: SharedApi["preflightStatus"] = (...args) =>
  current.preflightStatus(...args);

export const getTheme: SharedApi["getTheme"] = (...args) => current.getTheme(...args);

export const getLanguage: SharedApi["getLanguage"] = (...args) =>
  current.getLanguage(...args);

export const getWorkspaceRoot: SharedApi["getWorkspaceRoot"] = (...args) =>
  current.getWorkspaceRoot(...args);

export const getWorkspaceFiles: SharedApi["getWorkspaceFiles"] = (...args) =>
  current.getWorkspaceFiles(...args);

export const readWorkspaceFile: SharedApi["readWorkspaceFile"] = (...args) =>
  current.readWorkspaceFile(...args);

export const getSerialBuffer: SharedApi["getSerialBuffer"] = (...args) =>
  current.getSerialBuffer(...args);

// ----- The node's own inventory (shared names, v0.9 D2b-4b) -------------------
// These four are the desktop's commands too (`list_executors`, `list_sandboxes`,
// `current_sandbox`, `sandbox_candidates`), so they are *not* Web-only: both
// implementations carry them and the shared list above stays the check that they agree.

export const listExecutors: SharedApi["listExecutors"] = (...args) =>
  current.listExecutors(...args);

export const listSandboxes: SharedApi["listSandboxes"] = (...args) =>
  current.listSandboxes(...args);

export const currentSandbox: SharedApi["currentSandbox"] = (...args) =>
  current.currentSandbox(...args);

export const sandboxCandidates: SharedApi["sandboxCandidates"] = (...args) =>
  current.sandboxCandidates(...args);

// ----- Controls (implemented on the desktop; the Web client says so) ---------

export const setTheme: SharedApi["setTheme"] = (...args) => current.setTheme(...args);

export const setLanguage: SharedApi["setLanguage"] = (...args) =>
  current.setLanguage(...args);

export const setAuditAlert: SharedApi["setAuditAlert"] = (...args) =>
  current.setAuditAlert(...args);

export const runPreflight: SharedApi["runPreflight"] = (...args) =>
  current.runPreflight(...args);

export const acknowledgePreflight: SharedApi["acknowledgePreflight"] = (...args) =>
  current.acknowledgePreflight(...args);

export const setLlmConfig: SharedApi["setLlmConfig"] = (...args) =>
  current.setLlmConfig(...args);

export const loadStoredKey: SharedApi["loadStoredKey"] = (...args) =>
  current.loadStoredKey(...args);

export const clearLlmConfig: SharedApi["clearLlmConfig"] = (...args) =>
  current.clearLlmConfig(...args);

export const runAgent: SharedApi["runAgent"] = (...args) => current.runAgent(...args);

export const exportSerialLog: SharedApi["exportSerialLog"] = (...args) =>
  current.exportSerialLog(...args);

export const exportAuditJsonl: SharedApi["exportAuditJsonl"] = (...args) =>
  current.exportAuditJsonl(...args);

export const exportRunAudit: SharedApi["exportRunAudit"] = (...args) =>
  current.exportRunAudit(...args);

export const deleteSnapshot: SharedApi["deleteSnapshot"] = (...args) =>
  current.deleteSnapshot(...args);

export const saveSnapshotReal: SharedApi["saveSnapshotReal"] = (...args) =>
  current.saveSnapshotReal(...args);

export const resumeFromSnapshotReal: SharedApi["resumeFromSnapshotReal"] = (...args) =>
  current.resumeFromSnapshotReal(...args);

export const startToolchainDownload: SharedApi["startToolchainDownload"] = (...args) =>
  current.startToolchainDownload(...args);

export const cancelToolchainDownload: SharedApi["cancelToolchainDownload"] = (...args) =>
  current.cancelToolchainDownload(...args);

export const setQemuPath: SharedApi["setQemuPath"] = (...args) =>
  current.setQemuPath(...args);

export const clearQemuPath: SharedApi["clearQemuPath"] = (...args) =>
  current.clearQemuPath(...args);

export const setToolchainPath: SharedApi["setToolchainPath"] = (...args) =>
  current.setToolchainPath(...args);

export const clearToolchainPath: SharedApi["clearToolchainPath"] = (...args) =>
  current.clearToolchainPath(...args);

export const createSession: SharedApi["createSession"] = (...args) =>
  current.createSession(...args);

export const openSession: SharedApi["openSession"] = (...args) =>
  current.openSession(...args);

export const renameSession: SharedApi["renameSession"] = (...args) =>
  current.renameSession(...args);

export const deleteSession: SharedApi["deleteSession"] = (...args) =>
  current.deleteSession(...args);

export const clearAllSessions: SharedApi["clearAllSessions"] = (...args) =>
  current.clearAllSessions(...args);

// ----- Host events -----------------------------------------------------------

export const onHostEvent: SharedApi["onHostEvent"] = (...args) =>
  current.onHostEvent(...args);

export const onToolchainDownload: SharedApi["onToolchainDownload"] = (...args) =>
  current.onToolchainDownload(...args);

export const onAgentStreamDelta: SharedApi["onAgentStreamDelta"] = (...args) =>
  current.onAgentStreamDelta(...args);

export const onAgentStreamDone: SharedApi["onAgentStreamDone"] = (...args) =>
  current.onAgentStreamDone(...args);
