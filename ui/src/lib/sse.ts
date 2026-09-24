/**
 * The event stream's wire format (v0.9 D2b-3).
 *
 * Pure and dependency-free — text in, frames out; no `fetch`, no DOM — so
 * `ui/scripts/probe-ui-sse.mjs` imports the shipped module directly, like the other
 * `lib/` rules.
 *
 * The rules are the ones `docs/control-plane-events.md` settles and `cli/src/sse.rs`
 * implements on the Rust side: `id:` and `data:` lines, a blank line between frames,
 * comment lines (`: keep-alive`) that carry no frame, several `data:` lines in one
 * frame joined with newlines, and — at the end of a stream — a frame that was started
 * but never terminated still counts, because a server may omit the last blank line.
 *
 * Two things are deliberately **not** here. JSON: a frame's `data` is text, and which
 * envelope it holds is the transport's business (`api/http.ts`). And the field
 * prefixes are matched **without** requiring the space the spec makes optional
 * (`data:x` is a frame too), which is the one place this reader is looser than the
 * CLI's — that one can afford to be strict because it reads our own server.
 */

/** One frame: its `id:` (when it had one) and its joined `data:`. */
export interface SseFrame {
  id?: string;
  data: string;
}

/** One line, classified before it is folded into a frame. */
export type SseLine =
  | { kind: "id"; value: string }
  | { kind: "data"; value: string }
  | { kind: "comment" }
  | { kind: "empty" }
  | { kind: "other"; value: string };

/**
 * Classify one line, given without its terminator (a trailing `\r` is tolerated).
 *
 * A line with a field this reader does not know (`event:`, `retry:`) is `other` and
 * is ignored, which is what the format asks a reader to do with fields it has no use
 * for — it is not an error, and it does not end a frame.
 */
export function parseSseLine(raw: string): SseLine {
  const line = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
  if (line === "") return { kind: "empty" };
  if (line.startsWith(":")) return { kind: "comment" };
  if (line.startsWith("id:")) return { kind: "id", value: unpad(line.slice(3)) };
  if (line.startsWith("data:")) return { kind: "data", value: unpad(line.slice(5)) };
  return { kind: "other", value: line };
}

/** The spec's "remove one leading space, if there is one". */
function unpad(value: string): string {
  return value.startsWith(" ") ? value.slice(1) : value;
}

/**
 * Turns a stream of decoded text into frames.
 *
 * Chunk-based on purpose: a `ReadableStream` hands over whatever arrived, so a frame
 * (and even a single line) can straddle two chunks. What is left over is kept — that
 * is the `pending` a hand-written reader needs and the half of it people forget.
 */
export class SseReader {
  /** Text after the last newline: a line that has not finished arriving. */
  private pending = "";
  private id: string | undefined;
  private data: string[] = [];

  /** Feed decoded text; returns the frames it completed. */
  push(text: string): SseFrame[] {
    const frames: SseFrame[] = [];
    this.pending += text;
    let end = this.pending.indexOf("\n");
    while (end >= 0) {
      const line = this.pending.slice(0, end);
      this.pending = this.pending.slice(end + 1);
      const frame = this.consume(line);
      if (frame !== undefined) frames.push(frame);
      end = this.pending.indexOf("\n");
    }
    return frames;
  }

  /**
   * The frame the stream ended in the middle of, if any.
   *
   * A lone `id:` with no `data:` is not a frame — the CLI's reader makes the same
   * call, and it is the difference between "the stream stopped after a cursor" and
   * "the stream delivered something".
   */
  flush(): SseFrame | undefined {
    if (this.pending !== "") {
      this.consume(this.pending);
      this.pending = "";
    }
    return this.data.length === 0 ? undefined : this.takeFrame();
  }

  /** One line: fold it in, or hand back the frame it terminated. */
  private consume(rawLine: string): SseFrame | undefined {
    const line = parseSseLine(rawLine);
    switch (line.kind) {
      case "id":
        this.id = line.value;
        return undefined;
      case "data":
        this.data.push(line.value);
        return undefined;
      case "empty":
        return this.takeFrame();
      // A comment is a heartbeat, and an unknown field is not ours: neither ends a
      // frame, and neither is an error.
      default:
        return undefined;
    }
  }

  /** The frame assembled so far, cleared. `undefined` when nothing is buffered. */
  private takeFrame(): SseFrame | undefined {
    if (this.data.length === 0 && this.id === undefined) return undefined;
    const frame: SseFrame = { id: this.id, data: this.data.join("\n") };
    this.id = undefined;
    this.data = [];
    return frame;
  }
}
