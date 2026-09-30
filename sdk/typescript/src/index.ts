/**
 * `@riscdom/sdk` — a typed TypeScript client for the RiscDom control plane (v1.0 M7d).
 *
 * [docs/sdk.md](../../../docs/sdk.md) freezes what this is: **a thin, typed layer over the surface the
 * other documents already define**, which adds no semantics of its own. The HTTP surface is
 * [control-plane-api.md](../../../docs/control-plane-api.md) §5, the error model its §4, authentication
 * its §3, and the stream [control-plane-events.md](../../../docs/control-plane-events.md).
 *
 * **One package, browser and Node.** The runtime is `fetch` (a global in every current browser and in
 * Node ≥18), so this package has **no runtime dependency at all**. The stream is read with `fetch` and
 * a `ReadableStream` reader — **never `EventSource`**, which cannot set the `Authorization` header the
 * API needs ([control-plane-events.md](../../../docs/control-plane-events.md) §1).
 *
 * **The endpoint tables cannot drift from the server.** {@link QUERY_ENDPOINTS} and
 * {@link CONTROL_ENDPOINTS} are committed tables, and a test parses the marked blocks of
 * `docs/tool-schema-control-plane.md` and asserts the two are equal; those marked blocks are already
 * asserted to be exactly the server's own routes.
 */

/** One endpoint of the control plane, as the documents name it. */
export interface Endpoint {
  /** The tool name the tool-schema document gives it (`audit_status`, `runs`, …). */
  readonly tool: string;
  /** `GET` or `POST`. */
  readonly method: "GET" | "POST";
  /** The path, e.g. `/v0/audit/status`. */
  readonly path: string;
  /** The capability a caller must hold, e.g. `audit.read`. */
  readonly capability: string;
}

const query = (tool: string, path: string, capability: string): Endpoint => ({
  tool,
  method: "GET",
  path,
  capability,
});

const control = (tool: string, path: string, capability: string): Endpoint => ({
  tool,
  method: "POST",
  path,
  capability,
});

/**
 * The query endpoints (`GET`), exactly as the tool-schema document's `queries` marked block lists them
 * — which is exactly the server's own route table filtered to `GET`
 * ([control-plane-api.md](../../../docs/control-plane-api.md) §5.1).
 */
export const QUERY_ENDPOINTS: readonly Endpoint[] = [
  query("audit_status", "/v0/audit/status", "audit.read"),
  query("audit_events", "/v0/audit/events", "audit.read"),
  query("runs", "/v0/runs", "runs.read"),
  query("runs_diff", "/v0/runs/diff", "runs.read"),
  query("llm_provider_presets", "/v0/llm/provider-presets", "llm.read"),
  query("llm_config", "/v0/llm/config", "llm.read"),
  query("llm_readiness", "/v0/llm/readiness", "llm.read"),
  query("llm_local_probe", "/v0/llm/local-probe", "llm.read"),
  query("llm_stored_key", "/v0/llm/stored-key", "llm.read"),
  query("sessions", "/v0/sessions", "session.read"),
  query("sessions_current", "/v0/sessions/current", "session.read"),
  query("snapshots", "/v0/snapshots", "snapshot.read"),
  query("vm_running", "/v0/vm/running", "vm.read"),
  query("vm_status", "/v0/vm/status", "vm.read"),
  query("toolchain", "/v0/toolchain", "toolchain.read"),
  query("toolchain_download", "/v0/toolchain/download", "toolchain.read"),
  query("qemu", "/v0/qemu", "qemu.read"),
  query("qemu_status", "/v0/qemu/status", "qemu.read"),
  query("qemu_download", "/v0/qemu/download", "qemu.read"),
  query("preflight", "/v0/preflight", "preflight.read"),
  query("settings_theme", "/v0/settings/theme", "settings.read"),
  query("settings_language", "/v0/settings/language", "settings.read"),
  query("workspace_root", "/v0/workspace/root", "workspace.read"),
  query("workspace_files", "/v0/workspace/files", "workspace.read"),
  query("workspace_file", "/v0/workspace/file", "workspace.read"),
  query("serial", "/v0/serial", "serial.read"),
  query("sandboxes", "/v0/sandboxes", "sandbox.read"),
  query("sandboxes_current", "/v0/sandboxes/current", "sandbox.read"),
  query("sandboxes_candidates", "/v0/sandboxes/candidates", "sandbox.read"),
  query("sandboxes_requests", "/v0/sandboxes/requests", "sandbox.read"),
  query("resources", "/v0/resources", "vm.read"),
  query("executors", "/v0/executors", "agent.run"),
  query("capabilities", "/v0/capabilities", "status.read"),
  query("identity", "/v0/identity", "status.read"),
  query("peers", "/v0/peers", "status.read"),
  query("rooms", "/v0/rooms", "status.read"),
  query("connection", "/v0/connection", "status.read"),
  query("online", "/v0/online", "status.read"),
];

/**
 * The control endpoints (`POST`), exactly as the tool-schema document's `controls` marked block lists
 * them ([control-plane-api.md](../../../docs/control-plane-api.md) §5.2).
 */
export const CONTROL_ENDPOINTS: readonly Endpoint[] = [
  control("agent_run", "/v0/agent/run", "agent.run"),
  control("tasks", "/v0/tasks", "agent.run"),
  control("runs_export", "/v0/runs/export", "audit.export"),
  control("runs_abandon_stale", "/v0/runs/abandon-stale", "runs.control"),
  control("vm_start", "/v0/vm/start", "vm.control"),
  control("vm_stop", "/v0/vm/stop", "vm.control"),
  control("snapshots_save", "/v0/snapshots/save", "snapshot.write"),
  control("snapshots_resume", "/v0/snapshots/resume", "snapshot.write"),
  control("snapshots_delete", "/v0/snapshots/delete", "snapshot.write"),
  control("sessions_create", "/v0/sessions/create", "session.write"),
  control("sessions_open", "/v0/sessions/open", "session.write"),
  control("sessions_rename", "/v0/sessions/rename", "session.write"),
  control("sessions_delete", "/v0/sessions/delete", "session.write"),
  control("sessions_clear", "/v0/sessions/clear", "session.write"),
  control("toolchain_download_post", "/v0/toolchain/download", "toolchain.install"),
  control("toolchain_download_cancel", "/v0/toolchain/download/cancel", "toolchain.install"),
  control("qemu_download_post", "/v0/qemu/download", "qemu.configure"),
  control("qemu_download_cancel", "/v0/qemu/download/cancel", "qemu.configure"),
  control("toolchain_path", "/v0/toolchain/path", "toolchain.configure"),
  control("toolchain_path_clear", "/v0/toolchain/path/clear", "toolchain.configure"),
  control("qemu_path", "/v0/qemu/path", "qemu.configure"),
  control("qemu_path_clear", "/v0/qemu/path/clear", "qemu.configure"),
  control("preflight_run", "/v0/preflight/run", "preflight.run"),
  control("preflight_ack", "/v0/preflight/ack", "preflight.run"),
  control("audit_alert", "/v0/audit/alert", "settings.write"),
  control("audit_export", "/v0/audit/export", "audit.export"),
  control("settings_theme_post", "/v0/settings/theme", "settings.write"),
  control("settings_language_post", "/v0/settings/language", "settings.write"),
  control("llm_config_post", "/v0/llm/config", "llm.configure"),
  control("llm_stored_key_load", "/v0/llm/stored-key/load", "llm.configure"),
  control("llm_config_clear", "/v0/llm/config/clear", "llm.configure"),
  control("serial_export", "/v0/serial/export", "serial.export"),
  control("sandboxes_switch", "/v0/sandboxes/switch", "sandbox.switch"),
  control("sandboxes_requests_post", "/v0/sandboxes/requests", "agent.run"),
  control("workspace_import", "/v0/workspace/import", "workspace.write"),
  control("workspace_export", "/v0/workspace/export", "workspace.read"),
];

/** A query string as `(name, value)` pairs; a name may repeat (the stream's `event` filter does). */
export type Query = ReadonlyArray<readonly [string, string]>;

/** The parameters of the query endpoints, with the names the documents fix. */
export interface AuditEvents {
  limit: number;
  actor?: string;
  action_prefix?: string;
  from_ms?: number;
  to_ms?: number;
  from_id?: number;
  to_id?: number;
  before_id?: number;
}
export interface Runs {
  limit?: number;
}
export interface RunsDiff {
  run_a: string;
  run_b: string;
}
export interface LlmConfig {
  executor?: string;
}
export interface LlmReadiness {
  executor?: string;
}
export interface LlmStoredKey {
  provider_id: string;
  executor?: string;
}
export interface Sessions {
  limit?: number;
  executor?: string;
}
export interface SessionsCurrent {
  executor?: string;
}
export interface WorkspaceFile {
  path: string;
}
export interface SandboxesRequests {
  status?: string;
}

/** The bodies of the control endpoints that take one. */
export interface AgentRun {
  user_input: string;
  sandbox?: string;
  instance?: string;
}
export interface Tasks {
  target: string;
  input: string;
  sandbox?: string;
  instance?: string;
  id?: string;
}
export interface RunsExport {
  run_id: string;
  path: string;
}
export interface SnapshotName {
  name: string;
}
export interface SessionsCreate {
  title?: string;
  executor?: string;
}
export interface SessionsOpen {
  session_id: string;
  executor?: string;
}
export interface SessionsRename {
  session_id: string;
  title: string;
  executor?: string;
}
export interface SessionsDelete {
  session_id: string;
  executor?: string;
}
export interface SessionsClear {
  executor?: string;
}
export interface ToolchainDownload {
  toolchain?: string;
}
export interface PathArgument {
  path: string;
}
export interface AuditAlert {
  enabled: boolean;
}
export interface SettingsTheme {
  theme: string;
}
export interface SettingsLanguage {
  language: string;
}
export interface LlmConfigPost {
  api_key: string;
  base_url?: string;
  model?: string;
  provider_id?: string;
  remember?: boolean;
  executor?: string;
}
export interface LlmStoredKeyLoad {
  provider_id: string;
  executor?: string;
}
export interface LlmConfigClear {
  executor?: string;
}
export interface SandboxesSwitch {
  name: string;
}
export interface SandboxesRequestsPost {
  action: string;
  sandbox?: string;
  reason?: string;
}

/** Turn a parameters object into query pairs, skipping what is absent. */
function pairs(params: object): Query {
  const out: Array<readonly [string, string]> = [];
  for (const [name, value] of Object.entries(params)) {
    if (value !== undefined && value !== null) {
      out.push([name, String(value)]);
    }
  }
  return out;
}

function search(query: Query): string {
  if (query.length === 0) {
    return "";
  }
  const params = new URLSearchParams();
  for (const [name, value] of query) {
    params.append(name, value);
  }
  return `?${params.toString()}`;
}

/** The control plane's error object (api §4): the four fields every non-2xx answer carries. */
export interface ApiError {
  /** The HTTP status the server answered with. */
  readonly status: number;
  /** Stable, machine-readable. */
  readonly code: string;
  /** Human-readable, never a secret. */
  readonly message: string;
  /** Whether an identical retry can plausibly succeed. */
  readonly retryable: boolean;
  /** The offending input field or subsystem, when there is one. */
  readonly cause: string | null;
}

/** What a call can fail with: the transport, or the server's own answer. */
export class ClientError extends Error {
  /** `"transport"` when the request could not be completed, `"api"` when the server answered. */
  readonly kind: "transport" | "api";
  /** The error object, when the failure was the server's answer. */
  readonly api: ApiError | null;

  constructor(kind: "transport" | "api", message: string, api: ApiError | null = null) {
    super(message);
    this.name = "ClientError";
    this.kind = kind;
    this.api = api;
  }
}

/** Read an error object out of a body; a body that is not the documented shape still becomes one. */
function apiErrorFrom(status: number, text: string): ApiError {
  let body: Record<string, unknown> = {};
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed && typeof parsed === "object") {
      body = parsed as Record<string, unknown>;
    }
  } catch {
    // An unreadable body: report that, rather than pretending it said something.
  }
  const str = (key: string): string | null =>
    typeof body[key] === "string" ? (body[key] as string) : null;
  return {
    status,
    code: str("code") ?? "internal",
    message: str("message") ?? "the server answered with a body this client cannot read",
    retryable: body["retryable"] === true,
    cause: str("cause"),
  };
}

/** The stream's filters ([control-plane-events.md](../../../docs/control-plane-events.md) §4). */
export interface Filters {
  /** The event names to receive; absent means all. */
  events?: string[];
  /** One agent's events. */
  agent_id?: string;
  /** One task's events. */
  task_id?: string;
}

function filterQuery(filters: Filters): Query {
  const out: Array<readonly [string, string]> = [];
  for (const event of filters.events ?? []) {
    out.push(["event", event]);
  }
  if (filters.agent_id !== undefined) {
    out.push(["agent_id", filters.agent_id]);
  }
  if (filters.task_id !== undefined) {
    out.push(["task_id", filters.task_id]);
  }
  return out;
}

/** One frame as it arrived: the `id:` line (the replay cursor) and the `data:` payload. */
export interface Frame {
  /** The `id:` line. Opaque — store it and send it back, do not parse it (§1). */
  readonly id: string | null;
  /** The `data:` payload, exactly as it arrived. */
  readonly data: string;
}

/** The unified envelope every frame carries ([control-plane-events.md](../../../docs/control-plane-events.md) §2). */
export interface Envelope {
  readonly version: number;
  readonly kind: string;
  readonly event: string | null;
  readonly agent_id: string;
  readonly task_id: string | null;
  readonly ts: number;
  readonly payload: unknown;
}

/** The envelope's `kind`. A new value does not bump `version`, so `"unknown"` exists, not a throw. */
export type FrameKind = "event" | "hello" | "gap" | "unknown";

/** Parse a frame's payload as an envelope, when it is one. */
export function frameEnvelope(frame: Frame): Envelope | null {
  try {
    const parsed: unknown = JSON.parse(frame.data);
    if (parsed && typeof parsed === "object" && "kind" in parsed) {
      return parsed as Envelope;
    }
  } catch {
    // Not JSON: not an envelope.
  }
  return null;
}

/** The envelope's kind, as a value. */
export function frameKind(envelope: Envelope): FrameKind {
  switch (envelope.kind) {
    case "event":
      return "event";
    case "hello":
      return "hello";
    case "gap":
      return "gap";
    default:
      return "unknown";
  }
}

/**
 * A `gap` frame's `payload.lost_after`: the oldest id the server still holds. **A client that sees a
 * `gap` must re-sync from a query** — the stream cannot repair the hole, which is exactly why this is
 * an instruction and not an error ([control-plane-events.md](../../../docs/control-plane-events.md) §2).
 */
export function lostAfter(envelope: Envelope): string | null {
  const payload = envelope.payload;
  if (payload && typeof payload === "object" && "lost_after" in payload) {
    const value = (payload as Record<string, unknown>)["lost_after"];
    return typeof value === "string" ? value : null;
  }
  return null;
}

/**
 * A live event stream, read frame by frame.
 *
 * The bytes are read from `fetch`'s `ReadableStream` — never `EventSource`, which cannot carry the
 * `Authorization` header ([control-plane-events.md](../../../docs/control-plane-events.md) §1). Comment
 * lines (the 15-second `: keep-alive`) are skipped, and several `data:` lines in one frame are joined
 * with newlines, which is what the frame format says a reader should do.
 */
export class Subscription {
  private readonly reader: ReadableStreamDefaultReader<Uint8Array>;
  private readonly decoder = new TextDecoder();
  private buffer = "";
  private queued: Frame[] = [];
  /** The last `id:` seen — or the one the subscription resumed from, for `Last-Event-ID` (§1). */
  lastId: string | null;

  constructor(body: ReadableStream<Uint8Array>, lastEventId: string | null = null) {
    this.reader = body.getReader();
    this.lastId = lastEventId;
  }

  /** The next frame, or `null` at end of stream. */
  async next_frame(): Promise<Frame | null> {
    for (;;) {
      const queued = this.queued.shift();
      if (queued !== undefined) {
        return queued;
      }
      const chunk = await this.reader.read();
      if (chunk.done) {
        return null;
      }
      this.buffer += this.decoder.decode(chunk.value, { stream: true });
      const frames = this.drain();
      if (frames.length > 0) {
        this.queued = frames;
      }
    }
  }

  /** Every frame until the stream ends. */
  async *[Symbol.asyncIterator](): AsyncGenerator<Frame> {
    for (;;) {
      const frame = await this.next_frame();
      if (frame === null) {
        return;
      }
      yield frame;
    }
  }

  /** Close the stream. */
  async close(): Promise<void> {
    await this.reader.cancel();
  }

  /** Split every complete frame out of the buffer. */
  private drain(): Frame[] {
    const frames: Frame[] = [];
    for (;;) {
      const boundary = this.buffer.indexOf("\n\n");
      if (boundary < 0) {
        return frames;
      }
      const block = this.buffer.slice(0, boundary);
      this.buffer = this.buffer.slice(boundary + 2);
      let id: string | null = null;
      const data: string[] = [];
      for (const rawLine of block.split("\n")) {
        const line = rawLine.replace(/\r$/, "");
        if (line === "" || line.startsWith(":")) {
          continue; // a blank line between fields, or a keep-alive comment
        }
        if (line.startsWith("id:")) {
          id = line.slice(3).trimStart();
          continue;
        }
        if (line.startsWith("data:")) {
          data.push(line.slice(5).trimStart());
          continue;
        }
        // Any other field is ignored: the stream uses `id:` and `data:` only (§1).
      }
      if (id === null && data.length === 0) {
        continue;
      }
      const frame: Frame = { id, data: data.join("\n") };
      if (frame.id !== null) {
        this.lastId = frame.id;
      }
      frames.push(frame);
    }
  }
}

/**
 * A client for one node's control plane.
 *
 * The bearer token is the node's own (`<data-dir>/token`,
 * [control-plane-api.md](../../../docs/control-plane-api.md) §3), and it lives here so a request cannot
 * forget to carry it.
 */
export class Client {
  readonly baseUrl: string;
  private readonly token: string;
  private readonly agentName: string | null;
  private readonly fetchImpl: typeof fetch;

  constructor(
    baseUrl: string,
    token: string,
    options: { agentName?: string; fetch?: typeof fetch } = {},
  ) {
    this.baseUrl = baseUrl.replace(/\/+$/, "");
    this.token = token;
    this.agentName = options.agentName ?? null;
    this.fetchImpl = options.fetch ?? fetch;
  }

  /** `GET` a path with a query, and read the answer as JSON. */
  async get(path: string, query: Query = []): Promise<unknown> {
    const response = await this.send("GET", `${path}${search(query)}`, undefined, null);
    return readJson(response);
  }

  /** `GET` a path and read the answer as raw bytes. */
  async get_raw(path: string, query: Query = []): Promise<Uint8Array> {
    const response = await this.send("GET", `${path}${search(query)}`, undefined, null);
    return readBytes(response);
  }

  /** `POST` a JSON body and read a JSON answer. */
  async post(path: string, body: unknown): Promise<unknown> {
    const response = await this.send("POST", path, body, "application/json");
    return readJson(response);
  }

  /** `POST` with no body and read a JSON answer — most control endpoints. */
  async post_empty(path: string): Promise<unknown> {
    const response = await this.send("POST", path, undefined, null);
    return readJson(response);
  }

  /**
   * Subscribe to the event stream (`GET /v0/events`). `lastEventId` is the replay cursor: pass what
   * {@link Subscription.lastId} gave you to resume a dropped stream. If the server cannot replay that
   * far back, the first frame is a **`gap`** — an instruction to re-sync from a query, never an error.
   */
  async subscribe(filters: Filters = {}, lastEventId?: string): Promise<Subscription> {
    const headers: Record<string, string> = { Accept: "text/event-stream" };
    if (lastEventId !== undefined) {
      headers["Last-Event-ID"] = lastEventId;
    }
    const response = await this.send(
      "GET",
      `/v0/events${search(filterQuery(filters))}`,
      undefined,
      null,
      headers,
    );
    const body = response.body;
    if (body === null) {
      throw new ClientError("transport", "the stream answered with no body");
    }
    return new Subscription(body, lastEventId ?? null);
  }

  /** Build and send one request; a non-2xx answer becomes a {@link ClientError}. */
  private async send(
    method: "GET" | "POST",
    path: string,
    body: unknown,
    contentType: string | null,
    extraHeaders: Record<string, string> = {},
  ): Promise<Response> {
    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.token}`,
      ...extraHeaders,
    };
    if (this.agentName !== null) {
      headers["X-RiscDom-Agent"] = this.agentName;
    }
    if (contentType !== null) {
      headers["Content-Type"] = contentType;
    }
    const init: RequestInit = { method, headers };
    if (contentType !== null) {
      init.body = JSON.stringify(body);
    }
    let response: Response;
    try {
      response = await this.fetchImpl(`${this.baseUrl}${path}`, init);
    } catch (error) {
      throw new ClientError("transport", `the request could not be completed: ${String(error)}`);
    }
    if (!response.ok) {
      const text = await response.text();
      const api = apiErrorFrom(response.status, text);
      throw new ClientError("api", `${api.code} (HTTP ${api.status}): ${api.message}`, api);
    }
    return response;
  }

  /** `GET /v0/audit/status` — capability `audit.read`. */
  async audit_status(): Promise<unknown> {
    return this.get("/v0/audit/status");
  }

  /** `GET /v0/audit/events` — capability `audit.read`. */
  async audit_events(params: AuditEvents): Promise<unknown> {
    return this.get("/v0/audit/events", pairs({ ...params }));
  }

  /** `GET /v0/runs` — capability `runs.read`. */
  async runs(params: Runs = {}): Promise<unknown> {
    return this.get("/v0/runs", pairs({ ...params }));
  }

  /** `GET /v0/runs/diff` — capability `runs.read`. */
  async runs_diff(params: RunsDiff): Promise<unknown> {
    return this.get("/v0/runs/diff", pairs({ ...params }));
  }

  /** `GET /v0/llm/provider-presets` — capability `llm.read`. */
  async llm_provider_presets(): Promise<unknown> {
    return this.get("/v0/llm/provider-presets");
  }

  /** `GET /v0/llm/config` — capability `llm.read`. */
  async llm_config(params: LlmConfig = {}): Promise<unknown> {
    return this.get("/v0/llm/config", pairs({ ...params }));
  }

  /** `GET /v0/llm/readiness` — capability `llm.read`. */
  async llm_readiness(params: LlmReadiness = {}): Promise<unknown> {
    return this.get("/v0/llm/readiness", pairs({ ...params }));
  }

  /** `GET /v0/llm/local-probe` — capability `llm.read`. */
  async llm_local_probe(): Promise<unknown> {
    return this.get("/v0/llm/local-probe");
  }

  /** `GET /v0/llm/stored-key` — capability `llm.read`. */
  async llm_stored_key(params: LlmStoredKey): Promise<unknown> {
    return this.get("/v0/llm/stored-key", pairs({ ...params }));
  }

  /** `GET /v0/sessions` — capability `session.read`. */
  async sessions(params: Sessions = {}): Promise<unknown> {
    return this.get("/v0/sessions", pairs({ ...params }));
  }

  /** `GET /v0/sessions/current` — capability `session.read`. */
  async sessions_current(params: SessionsCurrent = {}): Promise<unknown> {
    return this.get("/v0/sessions/current", pairs({ ...params }));
  }

  /** `GET /v0/snapshots` — capability `snapshot.read`. */
  async snapshots(): Promise<unknown> {
    return this.get("/v0/snapshots");
  }

  /** `GET /v0/vm/running` — capability `vm.read`. */
  async vm_running(): Promise<unknown> {
    return this.get("/v0/vm/running");
  }

  /** `GET /v0/vm/status` — capability `vm.read`. */
  async vm_status(): Promise<unknown> {
    return this.get("/v0/vm/status");
  }

  /** `GET /v0/toolchain` — capability `toolchain.read`. */
  async toolchain(): Promise<unknown> {
    return this.get("/v0/toolchain");
  }

  /** `GET /v0/toolchain/download` — capability `toolchain.read`. */
  async toolchain_download(): Promise<unknown> {
    return this.get("/v0/toolchain/download");
  }

  /** `GET /v0/qemu` — capability `qemu.read`. */
  async qemu(): Promise<unknown> {
    return this.get("/v0/qemu");
  }

  /** `GET /v0/qemu/status` — capability `qemu.read`. */
  async qemu_status(): Promise<unknown> {
    return this.get("/v0/qemu/status");
  }

  /** `GET /v0/qemu/download` — capability `qemu.read`. */
  async qemu_download(): Promise<unknown> {
    return this.get("/v0/qemu/download");
  }

  /** `GET /v0/preflight` — capability `preflight.read`. */
  async preflight(): Promise<unknown> {
    return this.get("/v0/preflight");
  }

  /** `GET /v0/settings/theme` — capability `settings.read`. */
  async settings_theme(): Promise<unknown> {
    return this.get("/v0/settings/theme");
  }

  /** `GET /v0/settings/language` — capability `settings.read`. */
  async settings_language(): Promise<unknown> {
    return this.get("/v0/settings/language");
  }

  /** `GET /v0/workspace/root` — capability `workspace.read`. */
  async workspace_root(): Promise<unknown> {
    return this.get("/v0/workspace/root");
  }

  /** `GET /v0/workspace/files` — capability `workspace.read`. */
  async workspace_files(): Promise<unknown> {
    return this.get("/v0/workspace/files");
  }

  /** `GET /v0/workspace/file` — capability `workspace.read`. */
  async workspace_file(params: WorkspaceFile): Promise<unknown> {
    return this.get("/v0/workspace/file", pairs({ ...params }));
  }

  /** `GET /v0/serial` — capability `serial.read`. */
  async serial(): Promise<unknown> {
    return this.get("/v0/serial");
  }

  /** `GET /v0/sandboxes` — capability `sandbox.read`. */
  async sandboxes(): Promise<unknown> {
    return this.get("/v0/sandboxes");
  }

  /** `GET /v0/sandboxes/current` — capability `sandbox.read`. */
  async sandboxes_current(): Promise<unknown> {
    return this.get("/v0/sandboxes/current");
  }

  /** `GET /v0/sandboxes/candidates` — capability `sandbox.read`. */
  async sandboxes_candidates(): Promise<unknown> {
    return this.get("/v0/sandboxes/candidates");
  }

  /** `GET /v0/sandboxes/requests` — capability `sandbox.read`. */
  async sandboxes_requests(params: SandboxesRequests = {}): Promise<unknown> {
    return this.get("/v0/sandboxes/requests", pairs({ ...params }));
  }

  /** `GET /v0/resources` — reserved: answers `501`. */
  async resources(): Promise<unknown> {
    return this.get("/v0/resources");
  }

  /** `GET /v0/executors` — capability `agent.run`. */
  async executors(): Promise<unknown> {
    return this.get("/v0/executors");
  }

  /** `GET /v0/capabilities` — capability `status.read`. */
  async capabilities(): Promise<unknown> {
    return this.get("/v0/capabilities");
  }

  /** `GET /v0/identity` — capability `status.read`. */
  async identity(): Promise<unknown> {
    return this.get("/v0/identity");
  }

  /** `GET /v0/peers` — capability `status.read`. */
  async peers(): Promise<unknown> {
    return this.get("/v0/peers");
  }

  /** `GET /v0/rooms` — capability `status.read`. */
  async rooms(): Promise<unknown> {
    return this.get("/v0/rooms");
  }

  /** `GET /v0/connection` — capability `status.read`. */
  async connection(): Promise<unknown> {
    return this.get("/v0/connection");
  }

  /** `GET /v0/online` — capability `status.read`. */
  async online(): Promise<unknown> {
    return this.get("/v0/online");
  }

  /** `POST /v0/agent/run` — capability `agent.run`. */
  async agent_run(params: AgentRun): Promise<unknown> {
    return this.post("/v0/agent/run", params);
  }

  /** `POST /v0/tasks` — capability `agent.run`. */
  async tasks(params: Tasks): Promise<unknown> {
    return this.post("/v0/tasks", params);
  }

  /** `POST /v0/runs/export` — capability `audit.export`. */
  async runs_export(params: RunsExport): Promise<unknown> {
    return this.post("/v0/runs/export", params);
  }

  /** `POST /v0/runs/abandon-stale` — capability `runs.control`. */
  async runs_abandon_stale(): Promise<unknown> {
    return this.post_empty("/v0/runs/abandon-stale");
  }

  /** `POST /v0/vm/start` — capability `vm.control`; reserved, answers `501`. */
  async vm_start(): Promise<unknown> {
    return this.post_empty("/v0/vm/start");
  }

  /** `POST /v0/vm/stop` — capability `vm.control`. */
  async vm_stop(): Promise<unknown> {
    return this.post_empty("/v0/vm/stop");
  }

  /** `POST /v0/snapshots/save` — capability `snapshot.write`. */
  async snapshots_save(params: SnapshotName): Promise<unknown> {
    return this.post("/v0/snapshots/save", params);
  }

  /** `POST /v0/snapshots/resume` — capability `snapshot.write`. */
  async snapshots_resume(params: SnapshotName): Promise<unknown> {
    return this.post("/v0/snapshots/resume", params);
  }

  /** `POST /v0/snapshots/delete` — capability `snapshot.write`. */
  async snapshots_delete(params: SnapshotName): Promise<unknown> {
    return this.post("/v0/snapshots/delete", params);
  }

  /** `POST /v0/sessions/create` — capability `session.write`. */
  async sessions_create(params: SessionsCreate = {}): Promise<unknown> {
    return this.post("/v0/sessions/create", params);
  }

  /** `POST /v0/sessions/open` — capability `session.write`. */
  async sessions_open(params: SessionsOpen): Promise<unknown> {
    return this.post("/v0/sessions/open", params);
  }

  /** `POST /v0/sessions/rename` — capability `session.write`. */
  async sessions_rename(params: SessionsRename): Promise<unknown> {
    return this.post("/v0/sessions/rename", params);
  }

  /** `POST /v0/sessions/delete` — capability `session.write`. */
  async sessions_delete(params: SessionsDelete): Promise<unknown> {
    return this.post("/v0/sessions/delete", params);
  }

  /** `POST /v0/sessions/clear` — capability `session.write`. */
  async sessions_clear(params: SessionsClear = {}): Promise<unknown> {
    return this.post("/v0/sessions/clear", params);
  }

  /** `POST /v0/toolchain/download` — capability `toolchain.install`. */
  async toolchain_download_post(params: ToolchainDownload = {}): Promise<unknown> {
    return this.post("/v0/toolchain/download", params);
  }

  /** `POST /v0/toolchain/download/cancel` — capability `toolchain.install`. */
  async toolchain_download_cancel(): Promise<unknown> {
    return this.post_empty("/v0/toolchain/download/cancel");
  }

  /** `POST /v0/qemu/download` — capability `qemu.configure`. */
  async qemu_download_post(): Promise<unknown> {
    return this.post_empty("/v0/qemu/download");
  }

  /** `POST /v0/qemu/download/cancel` — capability `qemu.configure`. */
  async qemu_download_cancel(): Promise<unknown> {
    return this.post_empty("/v0/qemu/download/cancel");
  }

  /** `POST /v0/toolchain/path` — capability `toolchain.configure`. */
  async toolchain_path(params: PathArgument): Promise<unknown> {
    return this.post("/v0/toolchain/path", params);
  }

  /** `POST /v0/toolchain/path/clear` — capability `toolchain.configure`. */
  async toolchain_path_clear(): Promise<unknown> {
    return this.post_empty("/v0/toolchain/path/clear");
  }

  /** `POST /v0/qemu/path` — capability `qemu.configure`. */
  async qemu_path(params: PathArgument): Promise<unknown> {
    return this.post("/v0/qemu/path", params);
  }

  /** `POST /v0/qemu/path/clear` — capability `qemu.configure`. */
  async qemu_path_clear(): Promise<unknown> {
    return this.post_empty("/v0/qemu/path/clear");
  }

  /** `POST /v0/preflight/run` — capability `preflight.run`. */
  async preflight_run(): Promise<unknown> {
    return this.post_empty("/v0/preflight/run");
  }

  /** `POST /v0/preflight/ack` — capability `preflight.run`. */
  async preflight_ack(): Promise<unknown> {
    return this.post_empty("/v0/preflight/ack");
  }

  /** `POST /v0/audit/alert` — capability `settings.write`. */
  async audit_alert(params: AuditAlert): Promise<unknown> {
    return this.post("/v0/audit/alert", params);
  }

  /** `POST /v0/audit/export` — capability `audit.export`. */
  async audit_export(params: PathArgument): Promise<unknown> {
    return this.post("/v0/audit/export", params);
  }

  /** `POST /v0/settings/theme` — capability `settings.write`. */
  async settings_theme_post(params: SettingsTheme): Promise<unknown> {
    return this.post("/v0/settings/theme", params);
  }

  /** `POST /v0/settings/language` — capability `settings.write`. */
  async settings_language_post(params: SettingsLanguage): Promise<unknown> {
    return this.post("/v0/settings/language", params);
  }

  /** `POST /v0/llm/config` — capability `llm.configure`. */
  async llm_config_post(params: LlmConfigPost): Promise<unknown> {
    return this.post("/v0/llm/config", params);
  }

  /** `POST /v0/llm/stored-key/load` — capability `llm.configure`. */
  async llm_stored_key_load(params: LlmStoredKeyLoad): Promise<unknown> {
    return this.post("/v0/llm/stored-key/load", params);
  }

  /** `POST /v0/llm/config/clear` — capability `llm.configure`. */
  async llm_config_clear(params: LlmConfigClear = {}): Promise<unknown> {
    return this.post("/v0/llm/config/clear", params);
  }

  /** `POST /v0/serial/export` — capability `serial.export`. */
  async serial_export(params: PathArgument): Promise<unknown> {
    return this.post("/v0/serial/export", params);
  }

  /** `POST /v0/sandboxes/switch` — capability `sandbox.switch`. */
  async sandboxes_switch(params: SandboxesSwitch): Promise<unknown> {
    return this.post("/v0/sandboxes/switch", params);
  }

  /** `POST /v0/sandboxes/requests` — capability `agent.run`. */
  async sandboxes_requests_post(params: SandboxesRequestsPost): Promise<unknown> {
    return this.post("/v0/sandboxes/requests", params);
  }

  /**
   * `POST /v0/workspace/import` — capability `workspace.write`. The archive **is** the body (§5.2), so
   * this one takes bytes rather than an object; `force` replaces files that are already there.
   */
  async workspace_import(archive: Uint8Array, force = false): Promise<unknown> {
    const response = await this.postBytes(
      `/v0/workspace/import${force ? "?force=true" : ""}`,
      archive,
    );
    return readJson(response);
  }

  /**
   * `POST /v0/workspace/export` — capability `workspace.read`. **Bytes out, not JSON** (§5.2), so this
   * is the one control method that does not answer with parsed JSON.
   */
  async workspace_export(): Promise<Uint8Array> {
    const response = await this.send("POST", "/v0/workspace/export", undefined, null);
    return readBytes(response);
  }

  /** A `POST` whose body is raw bytes (the workspace archive, §5.2). */
  private async postBytes(path: string, body: Uint8Array): Promise<Response> {
    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.token}`,
      "Content-Type": "application/octet-stream",
    };
    if (this.agentName !== null) {
      headers["X-RiscDom-Agent"] = this.agentName;
    }
    let response: Response;
    try {
      response = await this.fetchImpl(`${this.baseUrl}${path}`, { method: "POST", headers, body });
    } catch (error) {
      throw new ClientError("transport", `the request could not be completed: ${String(error)}`);
    }
    if (!response.ok) {
      const text = await response.text();
      const api = apiErrorFrom(response.status, text);
      throw new ClientError("api", `${api.code} (HTTP ${api.status}): ${api.message}`, api);
    }
    return response;
  }
}

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (text.length === 0) {
    return null;
  }
  try {
    return JSON.parse(text) as unknown;
  } catch (error) {
    throw new ClientError("transport", `the answer was not JSON: ${String(error)}`);
  }
}

async function readBytes(response: Response): Promise<Uint8Array> {
  return new Uint8Array(await response.arrayBuffer());
}
