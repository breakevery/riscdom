/**
 * Preflight presentation (v0.4 batch 3).
 *
 * Dependency-free so `ui/scripts/probe-ui-preflight.mjs` can import the shipped
 * module directly: what the settings tab says about a preflight result is decided
 * here, not inside JSX. Its one import is the i18n registry, itself dependency-free
 * (v0.8 batch 2).
 */
import { t } from "../i18n/index.ts";
import type { StringKey } from "../i18n/index.ts";

export interface PreflightRowLike {
  step: string;
  /** `ok` / `failed` / `not_run`. */
  state: string;
  detail: string | null;
}

export interface PreflightLike {
  ran: boolean;
  checked: boolean;
  ok: boolean;
  rows: PreflightRowLike[];
  failed_step: string | null;
  detail: string | null;
  suggestion: string | null;
  overridden: boolean;
}

const STEP_LABELS: Record<string, StringKey | undefined> = {
  gcc_runs: "preflight.step.gcc_runs",
  gcc_compiles: "preflight.step.gcc_compiles",
  qemu_runs: "preflight.step.qemu_runs",
  guest_boots: "preflight.step.guest_boots",
};

/** Human label for a step id; an unknown id is shown verbatim. */
export function stepLabel(step: string): string {
  const key = STEP_LABELS[step];
  return key ? t(key) : step;
}

/** Mark for a row state; an unknown state is shown as "未检查". */
export function rowStateLabel(state: string): string {
  switch (state) {
    case "ok":
      return t("preflight.state.ok");
    case "failed":
      return t("preflight.state.failed");
    case "not_run":
      return t("preflight.state.not_run");
    default:
      return t("preflight.state.unchecked");
  }
}

/** One sentence for the top of the panel. */
export function preflightHeadline(p: PreflightLike | null): string {
  if (!p || !p.checked) {
    // v0.5 batch 11: the sentence used to quote the button's own label
    // (「重新预检」), which read like a link but was plain text. The button beside
    // the sentence is the only clickable thing, so the sentence points at it
    // instead of impersonating it.
    return t("preflight.headline.unchecked");
  }
  if (p.ok) {
    return t("preflight.headline.ok");
  }
  if (p.overridden) {
    return t("preflight.headline.overridden");
  }
  return t("preflight.headline.failed", { step: stepLabel(p.failed_step ?? "") });
}

/**
 * Offer the escape hatch when a check failed and the user has not already
 * accepted this configuration. A passing result has nothing to bypass, and an
 * unchecked one has nothing to bypass *yet*.
 */
export function shouldOfferOverride(p: PreflightLike | null): boolean {
  return Boolean(p && p.checked && !p.ok && !p.overridden);
}

/** The line shown while a check is running (fed by `preflight:progress`). */
export function progressLine(step: string | null, state: string | null): string {
  if (!step) return "";
  if (step === "done") {
    return state === "ok"
      ? t("preflight.progress.done_ok")
      : t("preflight.progress.done_failed");
  }
  const what = stepLabel(step);
  if (state === "running") return t("preflight.progress.running", { step: what });
  if (state === "ok") return t("preflight.progress.ok", { step: what });
  if (state === "failed") return t("preflight.progress.failed", { step: what });
  return what;
}
