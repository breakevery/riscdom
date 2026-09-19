import type { AppStore } from "../state/appStore";
import { defaultSnapshotName } from "../lib/snapshotName";

/**
 * Snapshots (real `tcp-relay` `.mig` files and legacy `reboot-fallback` `.json`).
 * The workspace file list lives here too, since both describe local state.
 */
export default function SnapshotTab({ store }: { store: AppStore }) {
  return (
    <>
      <div className="muted small">
        {store.snapshots.length} 个快照
        <button className="ghost tiny" onClick={() => void store.refreshSnapshots()}>
          刷新
        </button>
        <button
          className="ghost tiny"
          disabled={!store.vmIsRunning}
          title={
            store.vmIsRunning ? "保存当前 VM 状态" : "需要运行中的 VM（先让 AI 启动一台）"
          }
          onClick={() => {
            // v0.5 batch 11: a timestamp default instead of the old hard-coded
            // `snap1`, so pressing Enter does not reuse the same name every time.
            const name = window.prompt(
              "快照名称（字母/数字/-/_）",
              defaultSnapshotName(Date.now()),
            );
            if (name) void store.saveSnapshot(name.trim());
          }}
        >
          保存当前状态
        </button>
      </div>

      <div className="muted small">
        VM {store.vmIsRunning ? "运行中（host 持有，跨 run 复用）" : "未运行"}
        · 保存/恢复为真实快照（tcp-relay），需由 host 持有 VM。
      </div>

      <ul className="audit-list">
        {store.snapshots.map((s) => (
          <li key={s.name}>
            <span className="badge">{s.mode === "tcp-relay" ? "真实" : "重启式"}</span>
            <span className="action">{s.name}</span>
            <span className="muted small">{(s.size_bytes / 1024).toFixed(0)} KB</span>
            <span className="spacer" />
            {s.mode === "tcp-relay" ? (
              <button
                className="ghost tiny"
                onClick={() => {
                  if (window.confirm(`从快照“${s.name}”恢复？当前 VM 会被停止。`)) {
                    void store.resumeSnapshot(s.name);
                  }
                }}
              >
                恢复
              </button>
            ) : null}
            <button
              className="ghost tiny"
              onClick={() => {
                if (window.confirm(`删除快照“${s.name}”？`)) {
                  void store.deleteSnapshot(s.name);
                }
              }}
            >
              删除
            </button>
          </li>
        ))}
      </ul>

      <h3>工作区</h3>
      <div className="muted small">
        {store.workspaceFiles.length} 个文件
        <button className="ghost tiny" onClick={() => void store.refreshWorkspace()}>
          刷新
        </button>
      </div>
      <ul className="file-list">
        {store.workspaceFiles.slice(0, 12).map((f) => (
          <li key={f}>{f}</li>
        ))}
      </ul>
    </>
  );
}
