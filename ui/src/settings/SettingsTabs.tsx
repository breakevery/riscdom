import { useState } from "react";
import type { AppStore } from "../state/appStore";
import { t } from "../i18n/index.ts";
import type { StringKey } from "../i18n/index.ts";
import AppearanceTab from "./AppearanceTab";
import AuditTab from "./AuditTab";
import ModelTab from "./ModelTab";
import PluginTab from "./PluginTab";
import SnapshotTab from "./SnapshotTab";
import ToolchainTab from "./ToolchainTab";

type TabId = "model" | "toolchain" | "snapshot" | "audit" | "plugin" | "appearance";

const TABS: { id: TabId; labelKey: StringKey }[] = [
  { id: "model", labelKey: "settings.tab.model" },
  { id: "toolchain", labelKey: "settings.tab.toolchain" },
  { id: "snapshot", labelKey: "settings.tab.snapshot" },
  { id: "audit", labelKey: "settings.tab.audit" },
  { id: "plugin", labelKey: "settings.tab.plugin" },
  { id: "appearance", labelKey: "settings.tab.appearance" },
];

/**
 * Tab container for the settings page. Every tab stays mounted (hidden with
 * `display: none`) so switching tabs never loses half-filled forms.
 */
export default function SettingsTabs({ store }: { store: AppStore }) {
  const [tab, setTab] = useState<TabId>("model");
  const show = (id: TabId) => ({ display: tab === id ? undefined : "none" });

  return (
    <>
      <nav className="settings-tabs">
        {TABS.map((entry) => (
          <button
            key={entry.id}
            className={`tab-btn${tab === entry.id ? " active" : ""}`}
            onClick={() => setTab(entry.id)}
          >
            {t(entry.labelKey)}
          </button>
        ))}
      </nav>

      <div className="settings-body" style={show("model")}>
        <ModelTab store={store} />
      </div>
      <div className="settings-body" style={show("toolchain")}>
        <ToolchainTab store={store} />
      </div>
      <div className="settings-body" style={show("snapshot")}>
        <SnapshotTab store={store} />
      </div>
      <div className="settings-body" style={show("audit")}>
        <AuditTab store={store} />
      </div>
      <div className="settings-body" style={show("plugin")}>
        <PluginTab store={store} />
      </div>
      <div className="settings-body" style={show("appearance")}>
        <AppearanceTab store={store} />
      </div>
    </>
  );
}
