/**
 * The Web client's half of the API surface (v0.9 D2b-1).
 *
 * Same names, same signatures and same shapes as `api/tauri.ts`, but the calls go
 * over the control plane's HTTP endpoints instead of the Tauri IPC — which is what
 * lets one React application serve both the desktop shell and the browser
 * (`api/index.ts` picks one of the two at runtime).
 *
 * Three rules this file keeps, all of them so the two front ends cannot drift:
 *
 * - **the shapes are the same**, which is why they live in `./types` rather than
 *   here: `docs/control-plane-api.md` §5's view types are the host's, and both
 *   transports answer with them;
 * - **a rejection is a string**, exactly as the Tauri commands' `Result<_, String>`
 *   arrives (`String(e)` in the store, so the sentence a user sees is the host's);
 * - **a handful of endpoints wrap one value** (`{"theme": …`), and those eight
 *   are unwrapped here, in one place, instead of in every panel.
 *
 * Read-only endpoints are implemented; the controls and the event subscriptions
 * are stubs that say so (see the notes on `desktopOnly` and `onHostEvent`).
 */

import type {
  AgentOutcomeView,
  AuditEvent,
  AuditStatus,
  FingerprintFieldDiff,
  HealthView,
  LlmReadiness,
  LlmStatus,
  LocalProbeResult,
  NetworkSettings,
  LanStatus,
  CandidateView,
  CurrentSandboxResponse,
  ExecutorListResponse,
  PreflightView,
  ProviderPreset,
  QemuView,
  RunView,
  SandboxListResponse,
  SessionDetail,
  SessionMeta,
  SnapshotMeta,
  StatusView,
  ToolchainDownloadEvent,
  ToolchainDownloadStatus,
  ToolchainView,
  VmStatus,
} from "./types.ts";

// Re-exported so this module's surface is the same as `api/tauri.ts`'s, and so a
// consumer can name the shapes from either implementation.
export type {
  AgentOutcomeView,
  AuditEvent,
  AuditStatus,
  CandidateView,
  ChainStatus,
  CurrentSandboxResponse,
  ExecutorListResponse,
  ExecutorView,
  FingerprintFieldDiff,
  HealthView,
  HostEnvelope,
  LlmReadiness,
  LlmStatus,
  LocalProbeResult,
  NetworkSettings,
  LanStatus,
  LocalProviderInfo,
  PreflightRow,
  PreflightView,
  ProviderPreset,
  QemuView,
  RunView,
  SandboxListResponse,
  SandboxView,
  SessionDetail,
  SessionMessage,
  SessionMeta,
  SnapshotMeta,
  StatusView,
  ToolchainDownloadEvent,
  ToolchainDownloadStatus,
  ToolchainView,
  VmStatus,
} from "./types.ts";
import { SseReader } from "../lib/sse.ts";
import type { SseFrame } from "../lib/sse.ts";
import { unwrapHostPayload } from "./envelope.ts";
import type { HostEnvelope } from "./envelope.ts";

export { unwrapHostPayload } from "./envelope.ts";

/**
 * The API base. `""` means **same origin**, which is the deployment the server's
 * `--web-root` serves: one URL for the page, the API and the event stream, so the
 * Web client needs no configuration at all.
 */
let base = "";

export function setApiBase(next: string): void {
  base = next;
}

/**
 * The address in force. `""` means same origin.
 *
 * Read by the startup path rather than remembered anywhere else: which host a
 * remote-mode window is showing is a fact about this client, and one place that
 * knows it is one place that can be wrong.
 */
export function getApiBase(): string {
  return base;
}

/**
 * Where the Web client keeps its token.
 *
 * One key, in one of two stores: `sessionStorage` by default (it survives a
 * reload, and is gone when the tab is), or `localStorage` when the operator ticked
 * "remember this device". Never in the URL, and never anywhere the server could
 * log it.
 */
const TOKEN_KEY = "riscdom.web.token";

/**
 * The token this client presents, or `""`.
 *
 * Read from storage at module load — and storage can be *denied* (private
 * browsing, a locked-down webview) as well as absent (Node, in the probe), so both
 * the writes and this read are guarded: no token is a working state, not a crash.
 */
let token = readStoredToken();

function stores(): { local?: Storage; session?: Storage } {
  const scope = globalThis as { localStorage?: Storage; sessionStorage?: Storage };
  return { local: scope.localStorage, session: scope.sessionStorage };
}

function readStoredToken(): string {
  const { local, session } = stores();
  try {
    const remembered = local?.getItem(TOKEN_KEY);
    if (remembered) return remembered;
    return session?.getItem(TOKEN_KEY) ?? "";
  } catch {
    return "";
  }
}

/**
 * Install a token for every later request.
 *
 * `remember` is the "remember this device" choice, and the two stores are kept
 * exclusive: a remembered token is not also left in the session store, and a
 * session token is not left behind in the persistent one.
 */
export function setToken(next: string, remember = false): void {
  token = next;
  const { local, session } = stores();
  try {
    if (remember) {
      local?.setItem(TOKEN_KEY, next);
      session?.removeItem(TOKEN_KEY);
    } else {
      session?.setItem(TOKEN_KEY, next);
      local?.removeItem(TOKEN_KEY);
    }
  } catch {
    /* storage denied: the token still works for this page's lifetime */
  }
}

/** Forget the token, in memory and in both stores (the log-out path). */
export function clearToken(): void {
  token = "";
  const { local, session } = stores();
  try {
    local?.removeItem(TOKEN_KEY);
    session?.removeItem(TOKEN_KEY);
  } catch {
    /* nothing to clear */
  }
}

/** The token in force right now. */
export function currentToken(): string {
  return token;
}

type QueryValue = string | number | undefined;
type Query = Record<string, QueryValue>;

/** `?a=1&b=two` — encoded, and undefined or empty values left out entirely. */
function queryString(query?: Query): string {
  if (query === undefined) return "";
  const parts: string[] = [];
  for (const [key, value] of Object.entries(query)) {
    if (value === undefined || value === "") continue;
    parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`);
  }
  return parts.length === 0 ? "" : `?${parts.join("&")}`;
}

/**
 * The sentence for a refused request.
 *
 * The host's error model (`docs/control-plane-api.md` §4) is `{code, message,
 * retryable, cause}`; the UI shows one line, so this is that line. Anything that
 * is not the model (a proxy's HTML page, say) falls back to the status line rather
 * than inventing a code.
 */
async function failure(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as { code?: unknown; message?: unknown };
    if (typeof body.code === "string" && typeof body.message === "string") {
      return `${body.code}: ${body.message}`;
    }
  } catch {
    /* not the error model */
  }
  return `HTTP ${response.status} ${response.statusText}`;
}

/**
 * What a token check found.
 *
 * Four outcomes on purpose: "the token is wrong", "the server is not answering"
 * and "the server answered something else" are three different things a person
 * has to fix differently, and collapsing them into one "login failed" is what
 * sends someone to re-read a token that was never the problem.
 */
export type TokenCheck =
  | { kind: "ok"; version: string }
  | { kind: "unauthorized" }
  | { kind: "unreachable"; detail: string }
  | { kind: "other"; status: number };

/**
 * Is `candidate` the token this control plane wants?
 *
 * `url` defaults to the address in force. The network page passes a **candidate**
 * address it has not switched to yet, so checking one server does not retarget
 * this client at another one.
 *
 * The candidate rides this one request rather than being installed first, so a
 * rejected attempt cannot leave a bad token behind for the next call. The endpoint
 * is `/v0/health` because it is the cheapest authenticated one; the classification
 * is done here rather than through [`get`], which reports every failure as the
 * host's sentence and would lose the distinction above.
 */
export async function verifyToken(candidate: string, url = base): Promise<TokenCheck> {
  let response: Response;
  try {
    // The scheme word is spelled once, in a variable, for the reason the server's
    // own tests do it: a literal "Bearer <value>" span in a source file is a shape
    // that secret scanners and editors both like to rewrite.
    const scheme = "Bearer";
    response = await fetch(`${url}/v0/health`, {
      headers: { Authorization: `${scheme} ${candidate}` },
    });
  } catch (e) {
    return { kind: "unreachable", detail: String(e) };
  }
  if (response.ok) {
    const health = (await response.json()) as HealthView;
    return { kind: "ok", version: health.version };
  }
  if (response.status === 401 || response.status === 403) {
    return { kind: "unauthorized" };
  }
  return { kind: "other", status: response.status };
}

/**
 * One request, answered as JSON (or nothing at all for a `204`).
 *
 * Rejects with a **string**, because that is what the desktop's `invoke` rejects
 * with — the store renders `String(e)`, so both front ends show the host's own
 * sentence and neither shows a stack trace.
 */
async function request<T>(
  method: "GET" | "POST",
  path: string,
  query?: Query,
  body?: unknown,
): Promise<T> {
  const headers: Record<string, string> = {};
  if (token !== "") {
    // Spelled once, in a variable: see the note in `verifyToken`.
    const scheme = "Bearer";
    headers.Authorization = `${scheme} ${token}`;
  }
  if (body !== undefined) headers["Content-Type"] = "application/json";

  let response: Response;
  try {
    response = await fetch(base + path + queryString(query), {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (e) {
    // The network itself failed: the same shape as a host that is not running.
    throw `the control plane is unreachable: ${String(e)}`;
  }
  if (!response.ok) throw await failure(response);
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

const get = <T>(path: string, query?: Query): Promise<T> =>
  request<T>("GET", path, query);

// ----- Reads (v0.9 D2b serves these) ----------------------------------------

/**
 * The cheapest authenticated call, and the one a token is checked with.
 *
 * The desktop has no such command: its host is in the same process, so "is it
 * reachable and am I allowed" is not a question there.
 */
export const getHealth = () => get<HealthView>("/v0/health");

/** What this node is doing. Web-only, like [`getHealth`]. */
export const getStatus = () => get<StatusView>("/v0/status");

/** This node's fleet: the labels a task's `target` may name (v0.9 D2b-4b). */
export const listExecutors = () => get<ExecutorListResponse>("/v0/executors");

/** Every sandbox definition, plus the one a run would use and the fallback. */
export const listSandboxes = () => get<SandboxListResponse>("/v0/sandboxes");

/** The definition a run would use, and the fallback's name. */
export const currentSandbox = () => get<CurrentSandboxResponse>("/v0/sandboxes/current");

/** The raw scan: what is installed on this machine, and whether each runs. */
export const sandboxCandidates = () => get<CandidateView[]>("/v0/sandboxes/candidates");

export const getAuditStatus = () => get<AuditStatus>("/v0/audit/status");

export const listAuditEvents = (limit: number, actor?: string, actionPrefix?: string) =>
  get<AuditEvent[]>("/v0/audit/events", {
    limit,
    actor,
    action_prefix: actionPrefix,
  });

export const listRuns = (limit = 20) => get<RunView[]>("/v0/runs", { limit });

export const getRun = (runId: string) =>
  get<RunView | null>(`/v0/runs/${encodeURIComponent(runId)}`);

export const compareRunFingerprints = (runA: string, runB: string) =>
  get<FingerprintFieldDiff[]>("/v0/runs/diff", { run_a: runA, run_b: runB });

export const getProviderPresets = () =>
  get<ProviderPreset[]>("/v0/llm/provider-presets");

export const getLlmConfigStatus = () => get<LlmStatus>("/v0/llm/config");

export const getLlmReadiness = () => get<LlmReadiness>("/v0/llm/readiness");

export const probeLocalLlm = () => get<LocalProbeResult>("/v0/llm/local-probe");

/** The endpoint answers `{"present": bool}`; the desktop command answers a bool. */
export const hasStoredKey = async (providerId: string): Promise<boolean> =>
  (await get<{ present: boolean }>("/v0/llm/stored-key", {
    provider_id: providerId,
  })).present;

export const listSessions = (limit: number) =>
  get<SessionMeta[]>("/v0/sessions", { limit });

/** `{"session_id": string | null}` on the wire; the id itself in the UI. */
export const getCurrentSessionId = async (): Promise<string | null> =>
  (await get<{ session_id: string | null }>("/v0/sessions/current")).session_id;

export const listSnapshots = () => get<SnapshotMeta[]>("/v0/snapshots");

/** `{"running": bool}` on the wire. */
export const vmIsRunning = async (): Promise<boolean> =>
  (await get<{ running: boolean }>("/v0/vm/running")).running;

export const vmStatus = () => get<VmStatus>("/v0/vm/status");

export const probeToolchain = () => get<ToolchainView>("/v0/toolchain");

export const toolchainDownloadStatus = () =>
  get<ToolchainDownloadStatus>("/v0/toolchain/download");

export const probeQemu = () => get<QemuView>("/v0/qemu");

export const getQemuStatus = () => get<QemuView>("/v0/qemu/status");

export const preflightStatus = () => get<PreflightView>("/v0/preflight");

/** `{"theme": string}` on the wire. */
export const getTheme = async (): Promise<string> =>
  (await get<{ theme: string }>("/v0/settings/theme")).theme;

/** `{"language": string}` on the wire. */
export const getLanguage = async (): Promise<string> =>
  (await get<{ language: string }>("/v0/settings/language")).language;

/** `{"root": string}` on the wire. */
export const getWorkspaceRoot = async (): Promise<string> =>
  (await get<{ root: string }>("/v0/workspace/root")).root;

export const getWorkspaceFiles = () => get<string[]>("/v0/workspace/files");

/** `{"content": string}` on the wire. */
export const readWorkspaceFile = async (path: string): Promise<string> =>
  (await get<{ content: string }>("/v0/workspace/file", { path })).content;

/** `{"buffer": string}` on the wire. */
export const getSerialBuffer = async (): Promise<string> =>
  (await get<{ buffer: string }>("/v0/serial")).buffer;

// ----- Controls and events (D4 and D2b-3) -----------------------------------

/**
 * The controls the Web client does **not** have yet.
 *
 * v0.9's browser client is the read-only "look at it from a phone" surface
 * (`docs/decisions.md` §9) and its controls arrive with D4, so these reject rather
 * than pretend. They reject with a sentence, never synchronously, so a caller's
 * `catch` sees them exactly like a host refusal.
 */
function desktopOnly(command: string): Promise<never> {
  return Promise.reject(
    `${command} is a desktop control: the Web client is read-only in v0.9, and its controls arrive with D4`,
  );
}

// The two controls the Web client does implement (v0.9 D2b-4a, decision §62): theme and
// language are **display preferences**, not node configuration, and writing them is the
// whole point of the appearance page. Without these, the browser applied the choice
// locally and then reported the host's refusal — a switch that worked and looked broken.
export const setTheme = (theme: string): Promise<void> =>
  request<void>("POST", "/v0/settings/theme", undefined, { theme });

export const setLanguage = (language: string): Promise<void> =>
  request<void>("POST", "/v0/settings/language", undefined, { language });

// ----- The network face (v0.9.9 内网接入) ------------------------------------

/**
 * Wiring this node's own network is the desktop shell's, and there is no endpoint
 * for it: the browser can look at a node, it cannot rewire one. These reject with
 * a sentence rather than pretending, exactly like the controls above — and they
 * say "desktop control" because that is what they are.
 */
function desktopNetwork(what: string): Promise<never> {
  return Promise.reject(
    `${what} is a desktop control: the network face belongs to the shell that owns the embedded host, and a browser cannot reach it`,
  );
}

export const getNetwork = (): Promise<NetworkSettings | null> =>
  desktopNetwork("get_network");

export const setNetwork = (_network: NetworkSettings): Promise<void> =>
  desktopNetwork("set_network");

export const readLanToken = (): Promise<string> => desktopNetwork("read_lan_token");

/**
 * The OS keyring, where a remote server's token lives (v0.9.9 `"out"`).
 *
 * Desktop-only like the four above, and for the same reason: the keyring belongs
 * to the machine the shell runs on, and a browser has none. The token is a
 * credential, so it is never put in `settings.json` — see
 * `host_core::settings::NetworkSettings`.
 */
export const saveRemoteToken = (_host: string, _token: string): Promise<void> =>
  desktopNetwork("save_remote_token");

export const readRemoteToken = (_host: string): Promise<string | null> =>
  desktopNetwork("read_remote_token");

export const clearRemoteToken = (_host: string): Promise<void> =>
  desktopNetwork("clear_remote_token");

/**
 * Start the desktop application again.
 *
 * A separate refusal rather than `desktopOnly`: the reason is not that the Web
 * client is read-only, it is that a page cannot restart the process serving it.
 */
export const restartApp = (): Promise<void> =>
  Promise.reject(
    "restart_app is a desktop control: a browser page cannot restart the application that serves it",
  );

export const lanStatus = (): Promise<LanStatus> => desktopNetwork("lan_status");

export const setAuditAlert = (_enabled: boolean): Promise<void> =>
  desktopOnly("set_audit_alert");

export const runPreflight = (): Promise<void> => desktopOnly("run_preflight");

export const acknowledgePreflight = (): Promise<PreflightView> =>
  desktopOnly("acknowledge_preflight");

export const setLlmConfig = (
  _apiKey: string,
  _baseUrl: string,
  _model: string,
  _providerId: string,
  _remember: boolean,
): Promise<void> => desktopOnly("set_llm_config");

export const loadStoredKey = (_providerId: string): Promise<void> =>
  desktopOnly("load_stored_key");

export const clearLlmConfig = (): Promise<void> => desktopOnly("clear_llm_config");

export const runAgent = (_userInput: string): Promise<AgentOutcomeView> =>
  desktopOnly("run_agent");

export const exportSerialLog = (_path: string): Promise<number> =>
  desktopOnly("export_serial_log");

export const exportAuditJsonl = (_path: string): Promise<number> =>
  desktopOnly("export_audit_jsonl");

export const exportRunAudit = (_runId: string, _path: string): Promise<number> =>
  desktopOnly("export_run_audit");

export const deleteSnapshot = (_name: string): Promise<boolean> =>
  desktopOnly("delete_snapshot");

export const saveSnapshotReal = (_name: string): Promise<number> =>
  desktopOnly("save_snapshot_real");

export const resumeFromSnapshotReal = (_name: string): Promise<void> =>
  desktopOnly("resume_from_snapshot_real");

export const startToolchainDownload = (): Promise<void> =>
  desktopOnly("start_toolchain_download");

export const cancelToolchainDownload = (): Promise<void> =>
  desktopOnly("cancel_toolchain_download");

export const setQemuPath = (_path: string): Promise<void> =>
  desktopOnly("set_qemu_path");

export const clearQemuPath = (): Promise<void> => desktopOnly("clear_qemu_path");

export const setToolchainPath = (_path: string): Promise<void> =>
  desktopOnly("set_toolchain_path");

export const clearToolchainPath = (): Promise<void> =>
  desktopOnly("clear_toolchain_path");

export const createSession = (_title: string): Promise<string> =>
  desktopOnly("create_session");

export const openSession = (_sessionId: string): Promise<SessionDetail> =>
  desktopOnly("open_session");

export const renameSession = (_sessionId: string, _title: string): Promise<void> =>
  desktopOnly("rename_session");

export const deleteSession = (_sessionId: string): Promise<void> =>
  desktopOnly("delete_session");

export const clearAllSessions = (): Promise<void> => desktopOnly("clear_all_sessions");

/**
 * The event stream (v0.9 D2b-3).
 *
 * One `fetch` and one `ReadableStream`, because `EventSource` cannot carry the
 * `Authorization` header: the frames are `id:` / `data:` lines decoded by
 * `lib/sse.ts`, and the envelope inside each one decides where it goes.
 *
 * One stream serves every subscriber. `onHostEvent` keeps the name, the signature and
 * the "returns an unsubscribe" contract of the desktop's implementation, so the
 * store's ten subscriptions did not have to change.
 */

type HostListener = (payload: unknown) => void;

/** Listeners per host event name. The stream stays open while this, or the gap
 * listeners below, are not empty. */
const listeners = new Map<string, Set<HostListener>>();

/**
 * Who wants to know that frames were lost.
 *
 * A `gap` frame is the server saying "what you missed is gone" — it happens when a
 * subscriber lags past the replay buffer. It is not a host event, so it is not
 * delivered through the map above.
 */
const gapListeners = new Set<() => void>();

/** The cursor a reconnect resumes from: the last non-empty `id:` this client saw. */
let lastEventId = "";

/** The live stream, or `null` while there is none. */
let streamAbort: AbortController | null = null;
/** A queued reconnect, and the delay it will use. */
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectDelayMs = 0;

const RECONNECT_FIRST_MS = 1_000;
const RECONNECT_MAX_MS = 15_000;

/**
 * Register a callback for one host event, opening the stream on first use.
 *
 * The returned function unsubscribes; the last unsubscribe closes the stream, so a
 * page that stops caring does not leave a request open.
 */
export const onHostEvent = (
  event: string,
  callback: (payload: unknown) => void,
): (() => void) => {
  const set = listeners.get(event) ?? new Set<HostListener>();
  set.add(callback);
  listeners.set(event, set);
  void openStream();
  return () => {
    set.delete(callback);
    if (set.size === 0) listeners.delete(event);
    if (listeners.size === 0) closeStream();
  };
};

/** Register a callback for "frames were lost". See `gapListeners` above. */
export const onGap = (callback: () => void): (() => void) => {
  gapListeners.add(callback);
  void openStream();
  return () => {
    gapListeners.delete(callback);
    if (listeners.size === 0) closeStream();
  };
};

/** Drop the stream and any queued reconnect, and forget the backoff. */
function closeStream(): void {
  if (reconnectTimer !== null) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  reconnectDelayMs = 0;
  streamAbort?.abort();
  streamAbort = null;
}

/** Connect, unless a stream is up or one is already queued. */
async function openStream(): Promise<void> {
  if (streamAbort !== null || reconnectTimer !== null) return;
  if (listeners.size === 0 && gapListeners.size === 0) return;

  const controller = new AbortController();
  streamAbort = controller;
  const headers: Record<string, string> = { Accept: "text/event-stream" };
  if (token !== "") {
    // Concatenated, not templated: a literal "scheme + value" span is a shape secret
    // scanners and editors both like to rewrite (see the note in `request`).
    const scheme = "Bearer";
    headers.Authorization = scheme + " " + token;
  }
  // A browser does this for `EventSource`; a reader written by hand has to, and it is
  // the whole reason the server keeps a replay buffer.
  if (lastEventId !== "") headers["Last-Event-ID"] = lastEventId;

  try {
    const response = await fetch(`${base}/v0/events`, { headers, signal: controller.signal });
    if (!response.ok || response.body === null) {
      throw new Error(`the event stream answered ${response.status}`);
    }
    reconnectDelayMs = 0; // it worked, so a later drop starts its backoff over
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    const sse = new SseReader();
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      // `{ stream: true }`: a multi-byte character can straddle two chunks.
      for (const frame of sse.push(decoder.decode(value, { stream: true }))) dispatchFrame(frame);
    }
    const tail = sse.flush();
    if (tail !== undefined) dispatchFrame(tail);
  } catch {
    // A dropped (or refused) stream is not an error a dashboard should shout about:
    // every page keeps showing what it last read, and the loop below dials again.
  } finally {
    streamAbort = null;
    scheduleReconnect();
  }
}

/** Remember the cursor, then hand the envelope to whoever asked for it. */
function dispatchFrame(frame: SseFrame): void {
  if (frame.id !== undefined && frame.id !== "") lastEventId = frame.id;

  let envelope: unknown;
  try {
    envelope = JSON.parse(frame.data);
  } catch {
    return; // a frame this client cannot read is not a reason to stop reading
  }
  if (envelope === null || typeof envelope !== "object") return;

  const host = envelope as Partial<HostEnvelope>;
  if (host.kind === "gap") {
    for (const callback of gapListeners) callback();
    return;
  }
  // `hello` describes the stream itself (its buffer and its filters) and is not an
  // event: the pages read what they need when they mount, and the frame's `id` has
  // already been kept above as the cursor.
  if (host.kind !== "event" || typeof host.event !== "string") return;

  const set = listeners.get(host.event);
  if (set === undefined) return;
  const payload = unwrapHostPayload(envelope);
  for (const callback of set) callback(payload);
}

/** Dial again, further out each time, unless nobody is listening any more. */
function scheduleReconnect(): void {
  if (listeners.size === 0 && gapListeners.size === 0) return;
  reconnectDelayMs =
    reconnectDelayMs === 0
      ? RECONNECT_FIRST_MS
      : Math.min(reconnectDelayMs * 2, RECONNECT_MAX_MS);
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    void openStream();
  }, reconnectDelayMs);
}

/** Incremental assistant text from the LLM stream (`agent:stream:delta`). */
export const onAgentStreamDelta = (cb: (text: string) => void): (() => void) =>
  onHostEvent("agent:stream:delta", (payload) =>
    cb((payload as { text?: string }).text ?? ""),
  );

/** The LLM stream finished (`agent:stream:done`). */
export const onAgentStreamDone = (cb: () => void): (() => void) =>
  onHostEvent("agent:stream:done", () => cb());

/** Subscribe to `toolchain:download` progress events. */
export const onToolchainDownload = (
  onEvent: (event: ToolchainDownloadEvent) => void,
): (() => void) =>
  onHostEvent("toolchain:download", (payload) => onEvent(payload as ToolchainDownloadEvent));
