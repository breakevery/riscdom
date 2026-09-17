/**
 * Chat-log scroll policy (v0.3.1 #4).
 *
 * Kept as a pure module so the rule can be checked without a DOM: the chat panel
 * must behave like the serial canvas — a run that finishes while the reader has
 * scrolled up must **not** yank them back to the bottom.
 */

/** How close to the bottom still counts as "following". */
export const NEAR_BOTTOM_PX = 80;

export interface ScrollMetrics {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
}

/** Is the viewport close enough to the bottom to count as following? */
export function isNearBottom(m: ScrollMetrics): boolean {
  return m.scrollHeight - m.scrollTop - m.clientHeight <= NEAR_BOTTOM_PX;
}

export interface FinishDecision {
  /** Follow the newest output (scroll to the bottom)? */
  follow: boolean;
  /** Offer the "jump to latest" affordance instead? */
  offerJump: boolean;
}

/**
 * What a finished run does to the viewport.
 *
 * `stick` is the panel's "the reader is following the output" flag: when the
 * reader scrolled up, the finished run keeps the view where it is and offers the
 * jump button (only the button's presence changes, never the scrolling).
 */
export function onRunFinished(stick: boolean): FinishDecision {
  return stick ? { follow: true, offerJump: false } : { follow: false, offerJump: true };
}
