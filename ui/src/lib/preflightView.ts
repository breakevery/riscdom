/**
 * Preflight presentation (v0.4 batch 3).
 *
 * Dependency-free so `ui/scripts/probe-ui-preflight.mjs` can import the shipped
 * module directly: what the settings tab says about a preflight result is decided
 * here, not inside JSX.
 */

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

const STEP_LABELS: Record<string, string> = {
  gcc_runs: "工具链可运行",
  gcc_compiles: "编译最小 guest",
  qemu_runs: "QEMU 可运行",
  guest_boots: "guest 启动并回显 banner",
};

/** Human label for a step id; an unknown id is shown verbatim. */
export function stepLabel(step: string): string {
  return STEP_LABELS[step] ?? step;
}

/** Mark for a row state; an unknown state is shown as "未检查". */
export function rowStateLabel(state: string): string {
  switch (state) {
    case "ok":
      return "通过";
    case "failed":
      return "失败";
    case "not_run":
      return "未跑";
    default:
      return "未检查";
  }
}

/** One sentence for the top of the panel. */
export function preflightHeadline(p: PreflightLike | null): string {
  if (!p || !p.checked) {
    // v0.5 batch 11: the sentence used to quote the button's own label
    // (「重新预检」), which read like a link but was plain text. The button beside
    // the sentence is the only clickable thing, so the sentence points at it
    // instead of impersonating it.
    return "尚未预检：改完工具链 / QEMU 路径后会自动跑一次；右侧按钮可随时手动触发。";
  }
  if (p.ok) {
    return "预检通过：这套环境实际能编译并启动 guest。";
  }
  if (p.overridden) {
    return "预检未通过，但你已选择继续（该选择已记录）。";
  }
  return `预检未通过：卡在「${stepLabel(p.failed_step ?? "")}」。`;
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
    return state === "ok" ? "预检完成：全部通过。" : "预检完成：有步骤未通过。";
  }
  const what = stepLabel(step);
  if (state === "running") return `正在检查：${what}…`;
  if (state === "ok") return `已通过：${what}`;
  if (state === "failed") return `未通过：${what}`;
  return what;
}
