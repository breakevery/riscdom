import type { AppStore } from "../state/appStore";
import {
  comparePanelVisible,
  diffEmptyText,
  diffLoadingText,
  diffRowState,
  diffTitle,
  runsEmptyText,
  toCompareRows,
  toRunRows,
  valueText,
} from "../lib/runView";

/** Audit status: event count, hash-chain state, filtering and the recent list. */
export default function AuditTab({ store }: { store: AppStore }) {
  const chain = store.auditStatus?.chain;
  const runRows = toRunRows(store.runs, Date.now());
  const compareRows = toCompareRows(store.runs, store.selectedRuns);
  // The collapsed header of the field-level diff (v0.6 batch 1): the host's count
  // once it has answered, its refusal when it refused, and a loading line before
  // either. Never a count of rows the UI made up.
  const diffHeadline = store.diffRows
    ? diffTitle(store.diffRows)
    : (store.diffNote ?? diffLoadingText());

  return (
    <>
      <div className="status-line">
        <span className={`dot ${chain?.status === "Intact" ? "ok" : chain ? "bad" : "off"}`} />
        {store.auditStatus
          ? `${store.auditStatus.count} 条事件 · ${
              chain?.status === "Intact"
                ? `链完整 (${chain.length})`
                : chain
                  ? `链断裂 @${chain.at_id}`
                  : "未知"
            }`
          : "加载中…"}
        <button className="ghost tiny" onClick={() => void store.refreshAudit()}>
          刷新
        </button>
      </div>

      {/* v0.5 batch 11: the filter follows the input instead of applying on blur —
          waiting for focus to leave hid the effect of what had just been typed. */}
      <label className="row">
        <span className="small">按 actor 过滤</span>
        <input
          value={store.auditActorFilter}
          placeholder="sandbox / agent / host / human"
          onChange={(e) => {
            store.setAuditActorFilter(e.target.value);
            void store.refreshAudit(e.target.value);
          }}
        />
      </label>

      <ul className="audit-list">
        {store.auditEvents.map((e) => (
          <li key={e.id}>
            <span className="badge">{e.actor}</span>
            <span className="action">{e.action}</span>
            <span className="muted small">#{e.id}</span>
          </li>
        ))}
      </ul>

      {/* Runs (v0.4 1d): read-only window onto the derived index. */}
      <div className="status-line">
        <span className="small">最近运行（run）</span>
        <span className="muted small">勾选两个可并排对比指纹</span>
        <span className="spacer" />
        {store.selectedRuns.length > 0 ? (
          <button className="ghost tiny" onClick={() => store.clearRunSelection()}>
            清除选择
          </button>
        ) : null}
        <button className="ghost tiny" onClick={() => void store.refreshRuns()}>
          刷新
        </button>
      </div>
      {runRows.length === 0 ? (
        <div className="muted small">{runsEmptyText()}</div>
      ) : (
        <ul className="audit-list">
          {runRows.map((r) => (
            <li key={r.runId} title={r.runId}>
              <input
                type="checkbox"
                className="run-pick"
                checked={store.selectedRuns.includes(r.runId)}
                onChange={() => store.toggleRunSelection(r.runId)}
                aria-label={`选择 ${r.runId}`}
              />
              <span className={`badge ${r.status === "ok" ? "ok" : ""}`}>
                {r.statusLabel}
              </span>
              {/* The row must survive a narrow window (v0.5 batch 11): the text
                  spans shrink with an ellipsis, the button never does, and the
                  full value stays reachable as a tooltip. */}
              <span className="action" title={r.runId}>
                {r.fingerprint}
              </span>
              <span className="muted small">{r.whenLabel}</span>
              {r.parentLabel ? <span className="muted small">{r.parentLabel}</span> : null}
              {r.snapshotLabel ? (
                <span className="muted small">{r.snapshotLabel}</span>
              ) : null}
              <span className="spacer" />
              {r.exportable ? (
                <button
                  className="ghost tiny"
                  onClick={() => void store.exportRunAudit(r.runId)}
                >
                  导出
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      )}
      {/* Side-by-side (v0.5 batch 2, step 7): two runs, their digests next to each
          other. A field-by-field diff is v0.6. */}
      {comparePanelVisible(store.selectedRuns) ? (
        <>
          <div className="compare-grid">
            {compareRows.map((c) => (
              <div className="compare-col" key={c.runId} title={c.runId}>
                <div className="muted small">{c.runId}</div>
                <div className="status-line">
                  <span className={`badge ${c.status === "ok" ? "ok" : ""}`}>
                    {c.statusLabel}
                  </span>
                  <span className="muted small">{c.startedLabel}</span>
                </div>
                {c.snapshotLabel ? (
                  <div className="muted small">{c.snapshotLabel}</div>
                ) : null}
                <div className="action small">{c.fingerprintShort}</div>
                <div className="action small wrap">{c.fingerprint}</div>
              </div>
            ))}
          </div>
          {/* Field-level diff (v0.6 batch 1), under the two columns: the host's
              rows in the host's order — left is the first run selected, right the
              second — collapsed until asked for, and complete when opened (an
              unchanged field is dimmed, never dropped, never truncated). */}
          <details className="diff-block">
            <summary className="diff-summary">{diffHeadline}</summary>
            {store.diffRows && store.diffRows.length > 0 ? (
              <ul className="diff-list">
                {store.diffRows.map((row) => (
                  <li key={row.field} className={`diff-row ${diffRowState(row)}`}>
                    <span className="diff-field action small">{row.field}</span>
                    <pre className="diff-value">{valueText(row.a)}</pre>
                    <pre className="diff-value">{valueText(row.b)}</pre>
                  </li>
                ))}
              </ul>
            ) : null}
            {store.diffRows && store.diffRows.length === 0 ? (
              <div className="muted small">{diffEmptyText()}</div>
            ) : null}
          </details>
        </>
      ) : null}
      {store.runExportNote ? (
        <div className="muted small">{store.runExportNote}</div>
      ) : null}
    </>
  );
}
