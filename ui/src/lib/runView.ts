/**
 * Presentation helpers for the run list (v0.4 batch 1d).
 *
 * Kept dependency-free so `ui/scripts/probe-ui-runs.mjs` can import the shipped
 * module directly and check what the settings tab renders — there is no JS test
 * harness in this repository and new dependencies are off the table.
 */

/** The fields of a run the list needs (a subset of the backend `RunView`). */
export interface RunLike {
  run_id: string;
  status: string;
  /** Full configuration digest (64 hex characters). */
  fingerprint: string;
  fingerprint_short: string;
  parent_run_id: string | null;
  session_id: string | null;
  /** The snapshot the run was restored from, or null (v0.5 batch 3). */
  resumed_from_snapshot: string | null;
  started_at_ms: number;
  ended_at_ms: number | null;
}

/** One rendered row. */
export interface RunRow {
  runId: string;
  status: string;
  statusLabel: string;
  whenLabel: string;
  fingerprint: string;
  parentLabel: string;
  /** "Restored from <snapshot>", or empty for a run that started from scratch. */
  snapshotLabel: string;
  /**
   * May this run's audit interval be exported (v0.5 batches 1–2)?
   *
   * A run the chain has closed — by `run.end`, or by `host.run.abandoned` when its
   * process disappeared — has an interval, so the button is offered. A run that is
   * still **open** has none, and the host refuses it rather than exporting up to
   * wherever the chain happens to end, so the button is not offered either.
   */
  exportable: boolean;
}

const STATUS_LABELS: Record<string, string> = {
  open: "进行中",
  ok: "完成",
  failed: "失败",
  interrupted: "已中断",
  abandoned: "已放弃",
};

/** Human label for a run status; an unknown status is shown verbatim. */
export function statusLabel(status: string): string {
  return STATUS_LABELS[status] ?? status;
}

/** Coarse "when": the list only has to say how long ago a run started. */
export function formatWhen(ms: number, nowMs: number): string {
  const seconds = Math.max(0, Math.round((nowMs - ms) / 1000));
  if (seconds < 60) return `${seconds} 秒前`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  return `${Math.round(hours / 24)} 天前`;
}

/**
 * Rows for the run list, **newest first**.
 *
 * The backend returns runs oldest first (chain order); the panel shows the most
 * recent one at the top, which is also the order the operator thinks in.
 */
export function toRunRows(runs: RunLike[], nowMs: number): RunRow[] {
  return [...runs]
    .sort((a, b) => b.started_at_ms - a.started_at_ms)
    .map((run) => ({
      runId: run.run_id,
      status: run.status,
      statusLabel: statusLabel(run.status),
      whenLabel: formatWhen(run.started_at_ms, nowMs),
      fingerprint: run.fingerprint_short,
      parentLabel: run.parent_run_id ? `← ${run.parent_run_id}` : "",
      snapshotLabel: snapshotLabel(run.resumed_from_snapshot),
      exportable: run.status !== "open",
    }));
}

/** How a run's source snapshot is spelled; a run from scratch has none. */
export function snapshotLabel(snapshot: string | null): string {
  return snapshot ? `恢复自 ${snapshot}` : "";
}

/** What the list says when the log has no runs yet. */
export function runsEmptyText(): string {
  return "暂无运行记录：跑一次之后，这里会出现带配置指纹的 run。";
}

// ----- Audit export (v0.5 batch 2) -----------------------------------------

/**
 * Default file name for a run's audit export.
 *
 * The workspace root comes from the host — the UI does not hard-code where the
 * workspace is — and the file is named after the run so two exports never collide.
 * An unknown root falls back to the bare file name, which the save dialog still
 * resolves against the user's last directory.
 */
export function defaultExportPath(workspaceRoot: string | null, runId: string): string {
  const name = `${runId}.audit.jsonl`;
  if (!workspaceRoot) return name;
  const root = workspaceRoot.replace(/[\\/]+$/, "");
  const sep = root.includes("\\") ? "\\" : "/";
  return `${root}${sep}${name}`;
}

// ----- Side-by-side comparison (v0.5 batch 2) -------------------------------

/** How many runs the side-by-side panel holds. */
export const MAX_COMPARED_RUNS = 2;

/**
 * The selection after the operator clicked `runId`.
 *
 * Clicking a selected run removes it. Clicking a new one appends it, and **a third
 * selection replaces the oldest**: the panel then always shows the two runs the
 * operator touched last. Silently ignoring the third click would leave a stale pair
 * on screen and make "why did nothing happen?" a fair question; v0.5 only ever
 * compares two, so the oldest is the one to drop.
 */
export function toggleRunSelection(current: string[], runId: string): string[] {
  if (current.includes(runId)) return current.filter((id) => id !== runId);
  return [...current, runId].slice(-MAX_COMPARED_RUNS);
}

/** The side-by-side panel appears exactly when two runs are selected. */
export function comparePanelVisible(selected: string[]): boolean {
  return selected.length === MAX_COMPARED_RUNS;
}

/** Local absolute timestamp, `YYYY-MM-DD HH:MM:SS`. */
export function formatTimestamp(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return (
    `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ` +
    `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
  );
}

/** One column of the side-by-side panel. */
export interface CompareRow {
  runId: string;
  status: string;
  statusLabel: string;
  /** When the run started, as an absolute local timestamp. */
  startedLabel: string;
  /** The full digest (64 hex characters). */
  fingerprint: string;
  /** The first 16 characters, for scanning. */
  fingerprintShort: string;
  /** "Restored from <snapshot>", or empty. */
  snapshotLabel: string;
}

/**
 * Columns for the side-by-side panel, in **selection order** — the order the
 * operator clicked, so "left" and "right" mean what they just chose. A selected id
 * that is no longer in the list is dropped rather than rendered empty.
 */
export function toCompareRows(runs: RunLike[], selected: string[]): CompareRow[] {
  return selected
    .map((id) => runs.find((r) => r.run_id === id))
    .filter((run): run is RunLike => Boolean(run))
    .map((run) => ({
      runId: run.run_id,
      status: run.status,
      statusLabel: statusLabel(run.status),
      startedLabel: formatTimestamp(run.started_at_ms),
      fingerprint: run.fingerprint,
      fingerprintShort: run.fingerprint_short,
      snapshotLabel: snapshotLabel(run.resumed_from_snapshot),
    }));
}

// ----- Field-level fingerprint diff (v0.6 batch 1) --------------------------

/**
 * One top-level field of the two runs' fingerprints, as the host returns it.
 *
 * `a` / `b` are the documents' values — a whole nested object, not its keys — and
 * arrive already structured, which is why the UI can render them without parsing
 * anything. A field one document does not carry is `null` there.
 */
export interface FingerprintFieldDiff {
  field: string;
  a: unknown;
  b: unknown;
  is_different: boolean;
}

/**
 * How one value is shown: a string as it is, everything else as JSON.
 *
 * Nested values are printed whole — the diff compares them as one field, so
 * hiding part of one behind a disclosure would hide the difference itself. The
 * panel wraps and scrolls long text rather than truncating it.
 */
export function valueText(value: unknown): string {
  if (typeof value === "string") return value;
  if (value === undefined) return "null";
  return JSON.stringify(value, null, 2);
}

/** The collapsed header: how many fields there are, and how many differ. */
export function diffTitle(rows: FingerprintFieldDiff[]): string {
  const different = rows.filter((row) => row.is_different).length;
  return `字段级差异 · ${rows.length} 个字段 · ${different} 处不同`;
}

/** The row's state as a CSS hook. Never carries an order — the host's is kept. */
export function diffRowState(row: FingerprintFieldDiff): string {
  return row.is_different ? "different" : "same";
}

/** What the diff area says while the host has not answered yet. */
export function diffLoadingText(): string {
  return "差异读取中…";
}

/** What it says when there is no field to compare at all. */
export function diffEmptyText(): string {
  return "这两个 run 的指纹里没有可比字段。";
}

/** What it says when the host refused to produce the diff. */
export function diffFailedText(reason: string): string {
  return `指纹差异读取失败：${reason}`;
}
