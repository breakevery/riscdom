import { useEffect } from "react";
import type { AppStore } from "../../state/appStore";
import { t } from "../../i18n/index.ts";

/**
 * The fleet this node dispatches to (v0.9 D2b-4b).
 *
 * Read-only on purpose: sending a task to one of these is a control (§D4), so the board
 * shows *who* could take work and leaves the dispatch to the desktop. The list is the
 * `executors` block of `settings.json` — the node itself is deliberately not one of
 * them (`POST /v0/agent/run` is how you run on this node).
 */
export default function NodeExecutors({ store }: { store: AppStore }) {
  const { executors, refreshExecutors } = store;

  useEffect(() => {
    void refreshExecutors();
  }, [refreshExecutors]);

  return (
    <>
      <div className="row">
        <span className="muted small">{t("node.executors.count", { count: executors.length })}</span>
        <span className="spacer" />
        <button className="ghost tiny" onClick={() => void refreshExecutors()}>
          {t("status.refresh")}
        </button>
      </div>

      {executors.length === 0 ? (
        <div className="muted small">{t("node.executors.empty")}</div>
      ) : (
        <ul className="audit-list">
          {executors.map((executor) => (
            <li key={executor.agent_id}>
              <span className="action">{executor.agent_id}</span>
            </li>
          ))}
        </ul>
      )}

      <div className="muted small">{t("node.executors.note")}</div>
    </>
  );
}
