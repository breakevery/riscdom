import type { AppStore } from "../state/appStore";

/** Audit status: event count, hash-chain state, filtering and the recent list. */
export default function AuditTab({ store }: { store: AppStore }) {
  const chain = store.auditStatus?.chain;

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
    </>
  );
}
