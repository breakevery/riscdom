/**
 * Two-pane column-width policy (v0.3.1 #5).
 *
 * The chat column is user-draggable, but it must always leave room for the
 * serial column **inside the window**: a fixed upper bound (900 px) let the chat
 * column push the serial column off-screen in a narrow window.
 *
 * Pure module (no DOM) so the arithmetic can be checked directly.
 */

/** Width of the drag handle between the two columns (`.resizer` in the CSS). */
export const RESIZER_W = 6;

/** The serial column's preferred minimum (matches `minmax(260px, 1fr)`). */
export const MIN_SERIAL_W = 260;

/** Below this the serial column would be useless, so it stops shrinking. */
export const HARD_MIN_SERIAL_W = 120;

/**
 * The widest the chat column may be inside `containerW`.
 *
 * `containerW <= 0` means "not measured yet"; the caller's own maximum applies.
 */
export function maxChatWidth(
  containerW: number,
  minChatW: number,
  maxChatW: number,
): number {
  if (containerW <= 0) return maxChatW;
  return Math.max(minChatW, Math.min(maxChatW, containerW - RESIZER_W - MIN_SERIAL_W));
}

/** Clamp a desired chat width so the layout never overflows the window. */
export function clampChatWidth(
  desired: number,
  containerW: number,
  minChatW: number,
  maxChatW: number,
): number {
  return Math.max(minChatW, Math.min(maxChatWidth(containerW, minChatW, maxChatW), desired));
}

/**
 * The serial column's minimum inside this window.
 *
 * Normally {@link MIN_SERIAL_W}; in a window too small for both minima it
 * shrinks (but never below {@link HARD_MIN_SERIAL_W}) so the two columns always
 * add up to the available width.
 */
export function serialMinWidth(containerW: number, chatW: number): number {
  if (containerW <= 0) return MIN_SERIAL_W;
  return Math.max(
    HARD_MIN_SERIAL_W,
    Math.min(MIN_SERIAL_W, containerW - chatW - RESIZER_W),
  );
}
