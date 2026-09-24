/**
 * Status presentation (v0.9 D2b-2).
 *
 * Dependency-free beyond the i18n registry (itself dependency-free), so
 * `ui/scripts/probe-ui-status.mjs` can import the shipped module directly: what the
 * status page says about a node is decided here, not inside JSX.
 */
import { t } from "../i18n/index.ts";
import type { HealthView, StatusView } from "../api/types.ts";

/** One labelled line of the status page. */
export interface StatusField {
  label: string;
  value: string;
}

/** What a value that has not been read yet prints as. */
const NOT_READ = "—";

/**
 * How long the node has been up, in the largest unit that fits.
 *
 * The same shape the VM badge uses (`AppShell`'s `elapsedLabel`): hours carry one
 * decimal, so a node that has been up for days reads as "58.4" hours rather than a
 * day count — a status page asks how long this has been running.
 */
export function uptimeLabel(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return t("status.uptime_seconds", { n: seconds });
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return t("status.uptime_minutes", { n: minutes });
  return t("status.uptime_hours", { n: (minutes / 60).toFixed(1) });
}

/**
 * The node's own card, from `/v0/health`.
 *
 * `null` means the read has not answered yet; every field then prints the dash
 * rather than disappearing, so the page keeps its shape while it loads.
 */
export function nodeFields(health: HealthView | null): StatusField[] {
  return [
    { label: t("status.version"), value: health?.version ?? NOT_READ },
    {
      label: t("status.uptime"),
      value: health === null ? NOT_READ : uptimeLabel(health.uptime_ms),
    },
  ];
}

/**
 * The counters card, from `/v0/status`.
 *
 * `agents` is the number the host reports, which is `1` today on purpose — the node
 * answering is the only agent the control plane knows until the executor roster is
 * wired. The panel says so beside it (`status.agents_hint`) instead of leaving a
 * bare "1" to be misread as "one executor".
 */
export function counterFields(status: StatusView | null): StatusField[] {
  return [
    {
      label: t("status.connections"),
      value: status === null ? NOT_READ : String(status.connections),
    },
    {
      label: t("status.subscribers"),
      value: status === null ? NOT_READ : String(status.sse_subscribers),
    },
    {
      label: t("status.agents"),
      value: status === null ? NOT_READ : String(status.agents),
    },
    {
      label: t("status.agent_id"),
      value: status?.agent_id ?? NOT_READ,
    },
  ];
}
