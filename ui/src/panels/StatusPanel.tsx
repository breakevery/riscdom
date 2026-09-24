import { useEffect } from "react";
import type { AppStore } from "../state/appStore";
import { t } from "../i18n/index.ts";
import { counterFields, nodeFields } from "../lib/statusView";
import type { StatusField } from "../lib/statusView";

/**
 * What this node is doing (v0.9 D2b-2).
 *
 * The Web client's first page, and read-only by design: this is the "look at it
 * from a phone" surface. The values come from `/v0/health` and `/v0/status`, and the
 * wording — including the honest note about the `agents` count — is decided in
 * `lib/statusView.ts`, never here.
 */
export default function StatusPanel({ store }: { store: AppStore }) {
  const { refreshStatus, statusError, health, status } = store;

  // Read on arrival: the page has nothing to show before the first answer, and the
  // store is not asked to do this for every page (only this one needs it).
  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  const empty = health === null && status === null && statusError === null;

  return (
    <section className="panel">
      <header className="panel-head">
        {t("status.heading")}
        <span className="spacer" />
        <button className="ghost tiny" onClick={() => void refreshStatus()}>
          {t("status.refresh")}
        </button>
      </header>

      {statusError === null ? null : (
        <div className="banner warn">
          {t("status.unreachable")}: {statusError}
        </div>
      )}
      {empty ? <div className="muted small">{t("status.loading")}</div> : null}

      <div className="muted small">{t("status.node")}</div>
      {nodeFields(health).map((field) => (
        <Field key={field.label} field={field} />
      ))}

      <div className="muted small">{t("status.counters")}</div>
      {counterFields(status).map((field) => (
        <Field key={field.label} field={field} />
      ))}
      <div className="muted small">{t("status.agents_hint")}</div>
    </section>
  );
}

/** One label/value line. The value is the host's, printed as it arrived. */
function Field({ field }: { field: StatusField }) {
  return (
    <div className="row">
      <span className="muted small">{field.label}</span>
      <span>{field.value}</span>
    </div>
  );
}
