import type { AppStore } from "../state/appStore";
import { t } from "../i18n/index.ts";

/**
 * Placeholder for the capability-plugin system (v0.4+).
 *
 * The structure exists so the tab hierarchy is settled early; nothing here is
 * implemented yet, and no plugin can be installed from this build.
 */
export default function PluginTab(_props: { store: AppStore }) {
  return (
    <>
      <div className="banner info">{t("plugin.intro")}</div>
      <div className="muted small">{t("plugin.planned")}</div>
    </>
  );
}
