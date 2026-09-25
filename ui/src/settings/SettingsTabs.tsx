import { useState } from "react";
import type { AppStore } from "../state/appStore";
import { isTauriRuntime } from "../api";
import { DesktopOnly } from "../components/DesktopOnly";
import { t } from "../i18n/index.ts";
import type { StringKey } from "../i18n/index.ts";
import AppearanceTab from "./AppearanceTab";
import AuditTab from "./AuditTab";
import ModelTab from "./ModelTab";
import NetworkTab from "./NetworkTab";
import PluginTab from "./PluginTab";
import SnapshotTab from "./SnapshotTab";
import ToolchainTab from "./ToolchainTab";

type TabId =
  | "model"
  | "toolchain"
  | "snapshot"
  | "audit"
  | "plugin"
  | "appearance"
  | "network";

const TABS: { id: TabId; labelKey: StringKey }[] = [
  { id: "model", labelKey: "settings.tab.model" },
  { id: "toolchain", labelKey: "settings.tab.toolchain" },
  { id: "snapshot", labelKey: "settings.tab.snapshot" },
  { id: "audit", labelKey: "settings.tab.audit" },
  { id: "plugin", labelKey: "settings.tab.plugin" },
  { id: "appearance", labelKey: "settings.tab.appearance" },
  { id: "network", labelKey: "settings.tab.network" },
];

/**
 * Tab container for the settings page. Every tab stays mounted (hidden with
 * `display: none`) so switching tabs never loses half-filled forms.
 */
export default function SettingsTabs({ store }: { store: AppStore }) {
  // The browser does not get the model form (v0.9 D2b-4a): every submit on that screen
  // is a control, and its read-only half is not worth a second implementation. The
  // desktop shows all six tabs, exactly as before.
  const desktop = isTauriRuntime();
  // Two tabs are the desktop's alone: the model form (every submit on it is a
  // control) and the network face (the browser can look at a node, it cannot
  // rewire one). The desktop shows all seven.
  const tabs = desktop
    ? TABS
    : TABS.filter((entry) => entry.id !== "model" && entry.id !== "network");
  const [tab, setTab] = useState<TabId>(desktop ? "model" : "toolchain");
  const show = (id: TabId) => ({ display: tab === id ? undefined : "none" });

  return (
    <>
      <nav className="settings-tabs">
        {tabs.map((entry) => (
          <button
            key={entry.id}
            className={`tab-btn${tab === entry.id ? " active" : ""}`}
            onClick={() => setTab(entry.id)}
          >
            {t(entry.labelKey)}
          </button>
        ))}
      </nav>

      <DesktopOnly>
        <div className="settings-body" style={show("model")}>
          <ModelTab store={store} />
        </div>
      </DesktopOnly>
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
      <DesktopOnly>
        <div className="settings-body" style={show("network")}>
          <NetworkTab store={store} />
        </div>
      </DesktopOnly>
    </>
  );
}
