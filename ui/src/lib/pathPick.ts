/**
 * Path-picker helpers (v0.4 batch 2).
 *
 * Kept dependency-free so `ui/scripts/probe-ui-dialog.mjs` can import the shipped
 * module directly: the dialog call itself lives in the store, the decisions about
 * what came back live here.
 */

/** What the dialog plugin's `open()` can hand back. */
export type PickedPath = string | string[] | null;

export interface DialogFilter {
  name: string;
  extensions: string[];
}

/**
 * One path from a picker result.
 *
 * `null` means the user cancelled: nothing must be saved in that case (a cancelled
 * dialog is not a "clear the setting" instruction either). Several selections
 * collapse to the first — both callers ask for a single file.
 */
export function pickedPath(selected: PickedPath): string | null {
  const first = Array.isArray(selected) ? (selected.length > 0 ? selected[0] : null) : selected;
  if (!first) return null;
  const trimmed = first.trim();
  return trimmed.length > 0 ? trimmed : null;
}

/**
 * Filters for an executable picker.
 *
 * On Windows the binaries end in `.exe`, so the filter helps; elsewhere a filter
 * would hide the very file the user needs to pick, so there is none.
 */
export function executableFilters(label: string, platform: string): DialogFilter[] {
  return platform.toLowerCase().startsWith("win")
    ? [{ name: label, extensions: ["exe"] }]
    : [];
}
