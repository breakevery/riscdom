import type { AppStore } from "../state/appStore";
import SettingsTabs from "../settings/SettingsTabs";
import { t } from "../i18n/index.ts";

/**
 * The settings page. Reached from the gear in the top bar; `Esc` returns to the
 * main view. The chrome lives here, the tab contents live in `src/settings/`.
 */
export default function SettingsPanel({ store }: { store: AppStore }) {
  return (
    <section className="panel">
      <header className="panel-head">{t("settings.heading")}</header>
      <SettingsTabs store={store} />
    </section>
  );
}
