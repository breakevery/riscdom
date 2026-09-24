import { useEffect } from "react";
import type { AppStore } from "../../state/appStore";
import { t } from "../../i18n/index.ts";
import { counterFields, nodeFields } from "../../lib/statusView";
import type { StatusField } from "../../lib/statusView";

/**
 * The node's own card: `/v0/health` and `/v0/status` (v0.9 D2b-2; split out of the page
 * in D2b-4b). The wording — including the honest note about the `agents` count — is
 * decided in `lib/statusView.ts`, never here.
 */
export default function NodeStatus({ store }: { store: AppStore }) {
  const { refreshStatus, statusError, health, status } = store;

  // Read on arrival: the tab has nothing to show before the first answer, and the store
  // is not asked to do this for every tab (only this one needs it).
  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  const empty = health === null && status === null && statusError === null;

  return (
    <>
      <div className="row">
        <span className="spacer" />
        <button className="ghost tiny" onClick={() => void refreshStatus()}>
          {t("status.refresh")}
        </button>
      </div>

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
    </>
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
