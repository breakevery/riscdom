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
  fingerprint_short: string;
  parent_run_id: string | null;
  session_id: string | null;
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
  /**
   * May this run's audit interval be exported (v0.5 batch 1)?
   *
   * Only a run the chain has closed has an interval: `run.start` … `run.end`. An
   * open run (and an abandoned one, which the index deliberately leaves without an
   * end) has none, and the host refuses it rather than exporting up to wherever
   * the chain happens to end — so the button is not offered either.
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
      exportable: run.ended_at_ms !== null,
    }));
}

/** What the list says when the log has no runs yet. */
export function runsEmptyText(): string {
  return "暂无运行记录：跑一次之后，这里会出现带配置指纹的 run。";
}
