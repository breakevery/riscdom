import type { AppStore } from "../state/appStore";
import { DesktopOnly, WebOnly } from "../components/DesktopOnly";
import { defaultSnapshotName } from "../lib/snapshotName";
import { t } from "../i18n/index.ts";

/**
 * Snapshots (real `tcp-relay` `.mig` files and legacy `reboot-fallback` `.json`).
 * The workspace file list lives here too, since both describe local state.
 */
export default function SnapshotTab({ store }: { store: AppStore }) {
  return (
    <>
      {/* The browser is a read-only board (v0.9 D2b-4a). */}
      <WebOnly>
        <div className="muted small">{t("web.readonly_note")}</div>
      </WebOnly>

      <div className="muted small">
        {t("snapshot.count", { count: store.snapshots.length })}
        <button className="ghost tiny" onClick={() => void store.refreshSnapshots()}>
          {t("snapshot.refresh")}
        </button>
        <DesktopOnly>
          <button
            className="ghost tiny"
            disabled={!store.vmIsRunning}
            title={
              store.vmIsRunning
                ? t("snapshot.save_title")
                : t("snapshot.save_title_disabled")
            }
            onClick={() => {
              // v0.5 batch 11: a timestamp default instead of the old hard-coded
              // `snap1`, so pressing Enter does not reuse the same name every time.
              const name = window.prompt(
                t("snapshot.name_prompt"),
                defaultSnapshotName(Date.now()),
              );
              if (name) void store.saveSnapshot(name.trim());
            }}
          >
            {t("snapshot.save")}
          </button>
        </DesktopOnly>
      </div>

      <div className="muted small">
        VM {store.vmIsRunning ? t("snapshot.vm_running") : t("snapshot.vm_stopped")}
        · {t("snapshot.vm_note")}
      </div>

      <ul className="audit-list">
        {store.snapshots.map((s) => (
          <li key={s.name}>
            <span className="badge">
              {s.mode === "tcp-relay" ? t("snapshot.mode_real") : t("snapshot.mode_reboot")}
            </span>
            <span className="action">{s.name}</span>
            <span className="muted small">{(s.size_bytes / 1024).toFixed(0)} KB</span>
            <span className="spacer" />
            <DesktopOnly>
              {s.mode === "tcp-relay" ? (
                <button
                  className="ghost tiny"
                  onClick={() => {
                    if (window.confirm(t("snapshot.restore_confirm", { name: s.name }))) {
                      void store.resumeSnapshot(s.name);
                    }
                  }}
                >
                  {t("snapshot.restore")}
                </button>
              ) : null}
              <button
                className="ghost tiny"
                onClick={() => {
                  if (window.confirm(t("snapshot.delete_confirm", { name: s.name }))) {
                    void store.deleteSnapshot(s.name);
                  }
                }}
              >
                {t("snapshot.delete")}
              </button>
            </DesktopOnly>
          </li>
        ))}
      </ul>

      <h3>{t("snapshot.workspace")}</h3>
      <div className="muted small">
        {t("snapshot.files", { count: store.workspaceFiles.length })}
        <button className="ghost tiny" onClick={() => void store.refreshWorkspace()}>
          {t("snapshot.refresh")}
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
