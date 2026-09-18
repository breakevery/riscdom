import { useState } from "react";
import type { AppStore } from "../state/appStore";
import AppearanceTab from "./AppearanceTab";
import AuditTab from "./AuditTab";
import ModelTab from "./ModelTab";
import PluginTab from "./PluginTab";
import SnapshotTab from "./SnapshotTab";
import ToolchainTab from "./ToolchainTab";

type TabId = "model" | "toolchain" | "snapshot" | "audit" | "plugin" | "appearance";

const TABS: { id: TabId; label: string }[] = [
  { id: "model", label: "模型" },
  { id: "toolchain", label: "工具链" },
  { id: "snapshot", label: "快照" },
  { id: "audit", label: "审计" },
  { id: "plugin", label: "插件" },
  { id: "appearance", label: "外观" },
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
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`tab-btn${tab === t.id ? " active" : ""}`}
            onClick={() => setTab(t.id)}
          >
            {t.label}
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
