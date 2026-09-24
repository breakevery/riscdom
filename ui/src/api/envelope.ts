/**
 * The envelope every host event travels in (v0.9), and the one rule that reads
 * it.
 *
 * It lives beside the transport implementations rather than inside one of them:
 * the shape is the *host's*, not Tauri's or HTTP's, and both front ends have to
 * unwrap it exactly the same way.
 */

/**
 * The envelope every host event travels in (v0.9).
 *
 * `version` first, then `kind` / `event` / `agent_id` / `task_id` / `ts`, and the
 * panel's payload nested inside.
 */
export interface HostEnvelope {
  version: number;
  kind: string;
  event: string | null;
  agent_id: string;
  task_id: string | null;
  ts: number;
  payload: unknown;
}

/**
 * Unwrap a host event, so a panel callback keeps reading the payload it always
 * read. One boundary, one unwrap: the panels never see the envelope.
 */
export const unwrapHostPayload = (value: unknown): unknown => {
  if (value && typeof value === "object") {
    const candidate = value as Partial<HostEnvelope>;
    if (typeof candidate.version === "number" && "payload" in candidate) {
      return candidate.payload;
    }
  }
  return value;
};
