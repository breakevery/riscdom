import { useState } from "react";
import type { AppStore } from "../state/appStore";
import { isLocalHost, isRemote } from "../api";
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

/**
 * Which tabs a window that is **not** the local host keeps.
 *
 * A remote desktop and a browser are held to the same rule for the same reason:
 * they are reading another node over HTTP. `audit` is a reading of whatever node is
 * on screen; `appearance` writes this window's own display choice and nothing else.
 * The remote window keeps `network` because that page is where it goes back — the
 * browser has no such way round, and no embedded host to go back to.
 */
const REMOTE_TABS: TabId[] = ["audit", "appearance", "network"];
const WEB_TABS: TabId[] = ["audit", "appearance"];

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
  // Which tabs this window may use (v0.9.9). The browser is not offered the model
  // form — every submit on that screen is a control, and its read-only half is not
  // worth a second implementation — nor the network face, because it cannot rewire
  // a node. A desktop that connected to **another** node is a third case: it keeps
  // the network face (that is the way back to its own host) and loses the screens
  // that configure a node it is no longer looking at.
  const localHost = isLocalHost();
  const keep = localHost ? null : isRemote() ? REMOTE_TABS : WEB_TABS;
  const tabs = keep === null ? TABS : TABS.filter((entry) => keep.includes(entry.id));
  const [tab, setTab] = useState<TabId>(keep === null ? "model" : keep[0]);
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
