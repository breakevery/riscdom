/**
 * Default snapshot name (v0.5 batch 11).
 *
 * The name prompt used to open with the hard-coded example `snap1`, so an operator
 * who just pressed Enter gave every snapshot the same name — and a leftover
 * example name is not a name, it is a placeholder that ends up on disk.
 *
 * Kept dependency-free so `ui/scripts/probe-ui-snapshot.mjs` can import the shipped
 * module directly (there is no JS test harness in this repository).
 */

/**
 * `snap-YYYYMMDD-HHMM`, from the local clock.
 *
 * The backend accepts ASCII letters, digits, `-` and `_` only, up to 64 characters
 * (`validate_snapshot_name`), which this satisfies; sorting the names is also
 * chronological, because the format is big-endian down to the minute.
 */
export function defaultSnapshotName(nowMs: number): string {
  const d = new Date(nowMs);
  const p = (n: number) => String(n).padStart(2, "0");
  return (
    `snap-${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}` +
    `-${p(d.getHours())}${p(d.getMinutes())}`
  );
}
