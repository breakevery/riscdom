import { useState } from "react";
import type { AppStore } from "../state/appStore";
import { t } from "../i18n/index.ts";
import type { StringKey } from "../i18n/index.ts";
import NodeExecutors from "./node/NodeExecutors";
import NodeSandboxes from "./node/NodeSandboxes";
import NodeStatus from "./node/NodeStatus";

type NodeTab = "status" | "executors" | "sandboxes";

const NODE_TABS: { id: NodeTab; labelKey: StringKey }[] = [
  { id: "status", labelKey: "node.tab.status" },
  { id: "executors", labelKey: "node.tab.executors" },
  { id: "sandboxes", labelKey: "node.tab.sandboxes" },
];

/**
 * The node page (v0.9 D2b-2; three tabs since D2b-4b).
 *
 * The browser's first screen. Deliberately **not** more `AppShell` views: the node's
 * status, its fleet and its sandboxes are three faces of one subject, and a tab row is
 * *local* state where a view is global — decision §63. The row is the same
 * `.settings-tabs` / `.tab-btn` pair the settings page uses, so it needs no new styling;
 * it scrolls sideways when the screen is narrow.
 */
export default function StatusPanel({ store }: { store: AppStore }) {
  const [tab, setTab] = useState<NodeTab>("status");

  return (
    <section className="panel">
      <header className="panel-head">{t("status.heading")}</header>

      <nav className="settings-tabs" style={{ overflowX: "auto" }}>
        {NODE_TABS.map((entry) => (
          <button
            key={entry.id}
            className={`tab-btn${tab === entry.id ? " active" : ""}`}
            onClick={() => setTab(entry.id)}
          >
            {t(entry.labelKey)}
          </button>
        ))}
      </nav>

      {tab === "status" ? <NodeStatus store={store} /> : null}
      {tab === "executors" ? <NodeExecutors store={store} /> : null}
      {tab === "sandboxes" ? <NodeSandboxes store={store} /> : null}
    </section>
  );
}
