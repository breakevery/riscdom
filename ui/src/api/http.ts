/**
 * The Web client's half of the API surface (v0.9 D2b-1).
 *
 * Same names, same signatures and same shapes as `api/tauri.ts`, but the calls go
 * over the control plane's HTTP endpoints instead of the Tauri IPC 鈥?which is what
 * lets one React application serve both the desktop shell and the browser
 * (`api/index.ts` picks one of the two at runtime).
 *
 * Three rules this file keeps, all of them so the two front ends cannot drift:
 *
 * - **the shapes are the same**, which is why they live in `./types` rather than
 *   here: `docs/control-plane-api.md` 搂5's view types are the host's, and both
 *   transports answer with them;
 * - **a rejection is a string**, exactly as the Tauri commands' `Result<_, String>`
 *   arrives (`String(e)` in the store, so the sentence a user sees is the host's);
 * - **a handful of endpoints wrap one value** (`{"theme": 鈥`), and those eight
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
  LlmReadiness,
  LlmStatus,
  LocalProbeResult,
  PreflightView,
  ProviderPreset,
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

// Re-exported so this module's surface is the same as `api/tauri.ts`'s, and so a
// consumer can name the shapes from either implementation.
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
 * The bearer token this client presents, or `""`.
 *
 * In memory only, and deliberately: D2b-2 adds the login form and decides where a
 * token survives a reload. Nothing reads it before then, so a request made today
 * is simply the unauthenticated one the server refuses with `401`.
 */
let token = "";

export function setToken(next: string): void {
  token = next;
}

export function currentToken(): string {
  return token;
}

type QueryValue = string | number | undefined;
type Query = Record<string, QueryValue>;

/** `?a=1&b=two` 鈥?encoded, and undefined or empty values left out entirely. */
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
 * The host's error model (`docs/control-plane-api.md` 搂4) is `{code, message,
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
 * One request, answered as JSON (or nothing at all for a `204`).
 *
 * Rejects with a **string**, because that is what the desktop's `invoke` rejects
 * with 鈥?the store renders `String(e)`, so both front ends show the host's own
 * sentence and neither shows a stack trace.
 */
async function request<T>(
  method: "GET" | "POST",
  path: string,
  query?: Query,
  body?: unknown,
): Promise<T> {
  const headers: Record<string, string> = {};
  if (token !== "") headers.Authorization = `Bearer ${token}`;
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
 * (`docs/decisions.md` 搂9) and its controls arrive with D4, so these reject rather
 * than pretend. They reject with a sentence, never synchronously, so a caller's
 * `catch` sees them exactly like a host refusal.
 */
function desktopOnly(command: string): Promise<never> {
  return Promise.reject(
    `${command} is a desktop control: the Web client is read-only in v0.9, and its controls arrive with D4`,
  );
}

export const setTheme = (_theme: string): Promise<void> => desktopOnly("set_theme");

export const setLanguage = (_language: string): Promise<void> =>
  desktopOnly("set_language");

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
 * Subscriptions are the one stubbed group that must **not** reject: they are
 * registered from React effects, so a rejection would be an unhandled error on
 * every mount. The Web client's stream arrives with D2b-3 鈥?one `fetch` stream
 * feeding exactly this subscription shape 鈥?and until then this returns a no-op
 * unsubscribe, so a panel that only *subscribes* keeps working while the page
 * shows nothing live.
 *
 * (`unwrapHostPayload` above is already the real rule: the stream's frames carry
 * the same envelope the desktop's events do.)
 */
export const onHostEvent = (
  _event: string,
  _cb: (payload: unknown) => void,
): (() => void) => () => {};

export const onToolchainDownload = (
  _onEvent: (event: ToolchainDownloadEvent) => void,
): (() => void) => () => {};

export const onAgentStreamDelta = (_cb: (text: string) => void): (() => void) => () => {};

export const onAgentStreamDone = (_cb: () => void): (() => void) => () => {};
