import { useEffect } from "react";
import type { AppStore } from "../../state/appStore";
import { t } from "../../i18n/index.ts";

/**
 * Every sandbox definition this node knows, plus what a run would actually use
 * (v0.9 D2b-4b).
 *
 * Read-only: switching the node to another definition is a control (§D4). What the board
 * answers instead is "which definitions exist, which of them could run right now, and
 * which one is current" — which is the question a phone can usefully ask.
 */
export default function NodeSandboxes({ store }: { store: AppStore }) {
  const { sandboxes, candidates, refreshSandboxes, refreshCandidates } = store;

  useEffect(() => {
    void refreshSandboxes();
    void refreshCandidates();
  }, [refreshSandboxes, refreshCandidates]);

  const definitions = sandboxes?.sandboxes ?? [];
  const reload = () => {
    void refreshSandboxes();
    void refreshCandidates();
  };

  return (
    <>
      <div className="row">
        <span className="muted small">
          {t("node.sandboxes.count", { count: definitions.length })}
        </span>
        <span className="spacer" />
        <button className="ghost tiny" onClick={reload}>
          {t("status.refresh")}
        </button>
      </div>

      {definitions.length === 0 ? (
        <div className="muted small">{t("node.sandboxes.empty")}</div>
      ) : (
        <ul className="audit-list">
          {definitions.map((sandbox) => (
            <li key={sandbox.name}>
              <span className="action">{sandbox.name}</span>
              {sandbox.display_name ? (
                <span className="muted small">{sandbox.display_name}</span>
              ) : null}
              <span className="badge">
                {sandbox.source === "manual"
                  ? t("node.sandboxes.source_manual")
                  : t("node.sandboxes.source_discovered")}
              </span>
              <span className={`badge${sandbox.runnable ? " ok" : ""}`}>
                {sandbox.runnable
                  ? t("node.sandboxes.runnable")
                  : t("node.sandboxes.not_runnable")}
              </span>
              {sandboxes?.current === sandbox.name ? (
                <span className="muted small">{t("node.sandboxes.current")}</span>
              ) : null}
              {sandboxes?.default === sandbox.name ? (
                <span className="muted small">{t("node.sandboxes.default")}</span>
              ) : null}
              {sandbox.shadowed ? (
                <span className="muted small">{t("node.sandboxes.shadowed")}</span>
              ) : null}
              <span className="spacer" />
              <details>
                <summary className="muted small">{t("node.sandboxes.details")}</summary>
                <div className="muted small">
                  {sandbox.memory_mb === null ? null : `${sandbox.memory_mb} MB · `}
                  {sandbox.kernel ?? "—"} · {sandbox.qemu_exe ?? "—"} ·{" "}
                  {sandbox.toolchain_path ?? "—"}
                  {sandbox.notes ? ` · ${sandbox.notes}` : ""}
                </div>
              </details>
            </li>
          ))}
        </ul>
      )}

      <details>
        <summary className="muted small">
          {t("node.candidates.heading", { count: candidates.length })}
        </summary>
        {candidates.length === 0 ? (
          <div className="muted small">{t("node.candidates.empty")}</div>
        ) : (
          <ul className="audit-list">
            {candidates.map((candidate) => (
              <li key={candidate.path}>
                <span className="badge">{candidate.kind}</span>
                <span className="muted small">{candidate.version}</span>
                <span className="action">{candidate.path}</span>
                <span className="muted small">{candidate.origin}</span>
              </li>
            ))}
          </ul>
        )}
      </details>

      <div className="muted small">{t("node.sandboxes.note")}</div>
    </>
  );
}
