import type { AppStore } from "../state/appStore";
import { runsEmptyText, toRunRows } from "../lib/runView";

/** Audit status: event count, hash-chain state, filtering and the recent list. */
export default function AuditTab({ store }: { store: AppStore }) {
  const chain = store.auditStatus?.chain;
  const runRows = toRunRows(store.runs, Date.now());

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

      <label className="row">
        <span className="small">按 actor 过滤</span>
        <input
          value={store.auditActorFilter}
          placeholder="sandbox / agent / host / human"
          onChange={(e) => store.setAuditActorFilter(e.target.value)}
          onBlur={() => void store.refreshAudit()}
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
              <span className={`badge ${r.status === "ok" ? "ok" : ""}`}>
                {r.statusLabel}
              </span>
              <span className="action">{r.fingerprint}</span>
              <span className="muted small">{r.whenLabel}</span>
              {r.parentLabel ? <span className="muted small">{r.parentLabel}</span> : null}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
