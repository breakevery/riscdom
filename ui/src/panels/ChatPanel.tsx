import { useEffect, useRef, useState } from "react";
import * as api from "../api/tauri";
import { isNearBottom as metricsNearBottom, onRunFinished } from "../lib/scrollRule";
import { relTime, type AppStore } from "../state/appStore";
import { t } from "../i18n/index.ts";

export default function ChatPanel({ store }: { store: AppStore }) {
  const [input, setInput] = useState("");
  const [showSessions, setShowSessions] = useState(false);
  const canSend = !store.busy && input.trim().length > 0;

  // ----- auto-scroll (v0.3 #1) --------------------------------------------
  const logRef = useRef<HTMLDivElement | null>(null);
  const stickRef = useRef(true); // following the latest output?
  const rafRef = useRef<number | null>(null);
  const [showJump, setShowJump] = useState(false);

  const isNearBottom = () => {
    const el = logRef.current;
    if (!el) return true;
    return metricsNearBottom(el);
  };

  const scrollToBottom = () => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  };

  /** Coalesce bursts (streaming deltas) into one follow per frame. */
  const scheduleFollow = () => {
    if (rafRef.current !== null) return;
    rafRef.current = window.requestAnimationFrame(() => {
      rafRef.current = null;
      if (stickRef.current) scrollToBottom();
      else setShowJump(true);
    });
  };

  const onScroll = () => {
    const near = isNearBottom();
    stickRef.current = near;
    if (near) setShowJump(false);
  };

  const jumpToLatest = () => {
    stickRef.current = true;
    setShowJump(false);
    scrollToBottom();
  };

  // New messages and streaming text both land here.
  useEffect(() => {
    scheduleFollow();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [store.messages, store.streaming]);

  // A finished run keeps following only when the reader was still following;
  // otherwise the viewport stays where they left it and the jump button shows.
  // Same rule as the serial panel (v0.3.1 #4).
  useEffect(() => {
    if (store.busy) return;
    const decision = onRunFinished(stickRef.current);
    if (decision.follow) {
      setShowJump(false);
      scrollToBottom();
    } else {
      setShowJump(decision.offerJump);
    }
  }, [store.busy]);

  // Switching sessions starts at the bottom.
  useEffect(() => {
    stickRef.current = true;
    setShowJump(false);
    scrollToBottom();
  }, [store.currentSessionId]);

  useEffect(
    () => () => {
      if (rafRef.current !== null) window.cancelAnimationFrame(rafRef.current);
    },
    [],
  );

  const submit = async () => {
    if (!canSend) return;
    const text = input;
    setInput("");

    // No-key fallback: check readiness before calling the backend.
    try {
      const readiness = await api.getLlmReadiness();
      if (!readiness.ready) {
        store.pushSystem(
          readiness.suggestion ?? t("chat.not_ready"),
        );
        return;
      }
    } catch {
      // If readiness cannot be queried, fall through and let run_agent report.
    }

    await store.send(text);
  };

  return (
    <section className="panel">
      <header className="panel-head chat-head">
        <span>{t("chat.heading")}</span>
        <button className="ghost tiny" onClick={() => setShowSessions((v) => !v)}>
          {t("chat.sessions", { count: store.sessions.length })}
        </button>
      </header>

      {store.toolchainMissing || store.toolchainDownload.in_progress ? (
        <div className="banner warn">
          {store.toolchainDownload.in_progress ? (
            <span>
              {t("chat.toolchain_downloading")}
              {store.toolchainDownload.progress
                ? ` ${Math.min(
                    100,
                    Math.round(
                      (store.toolchainDownload.progress.downloaded /
                        (store.toolchainDownload.progress.total ?? 1)) *
                        100,
                    ),
                  )}%`
                : ""}
            </span>
          ) : (
            <span>{t("chat.toolchain_missing")}</span>
          )}
        </div>
      ) : null}

      {showSessions ? (
        <div className="session-list">
          <div className="row">
            <button className="ghost tiny" onClick={() => void store.newSession()}>
              {t("chat.session_new")}
            </button>
            <button className="ghost tiny" onClick={() => void store.refreshSessions()}>
              {t("chat.refresh")}
            </button>
          </div>
          {store.sessions.length === 0 ? (
            <div className="muted small">{t("chat.sessions_empty")}</div>
          ) : null}
          <ul>
            {store.sessions.map((s) => (
              <li key={s.id} className={s.id === store.currentSessionId ? "active" : ""}>
                <button
                  className="link"
                  title={s.id}
                  onClick={() => {
                    void store.openSession(s.id);
                    setShowSessions(false);
                  }}
                >
                  {s.title}
                </button>
                <span className="muted small">
                  {relTime(s.updated_at_ms)} · {s.message_count}
                </span>
                <span className="spacer" />
                <button
                  className="ghost tiny"
                  onClick={() => {
                    const next = window.prompt(t("chat.rename_prompt"), s.title);
                    if (next && next.trim()) void store.renameSession(s.id, next.trim());
                  }}
                >
                  {t("chat.rename")}
                </button>
                <button
                  className="ghost tiny"
                  onClick={() => {
                    if (
                      window.confirm(
                        t("chat.delete_confirm", { title: s.title }),
                      )
                    ) {
                      void store.deleteSession(s.id);
                    }
                  }}
                >
                  {t("chat.delete")}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="scroll-wrap">
        <div className="chat-log" ref={logRef} onScroll={onScroll}>
          {store.messages.length === 0 ? (
            <div className="muted small">{t("chat.empty_hint")}</div>
          ) : null}

          {store.messages.map((m) =>
            m.role === "tool" ? (
              <details key={m.id} className="msg tool">
                <summary>
                  {/* v0.5 batch 11: these markers were literal `?` in the source
                      (the emoji they replaced were lost when the line was written),
                      so the row read `?? write_source ?`. A status dot from CSS plus
                      a word cannot be lost or fail to render. */}
                  <span
                    className={`dot ${
                      m.ok === undefined ? "off" : m.ok ? "ok" : "bad"
                    }`}
                  />
                  {t("chat.tool", { name: m.toolName ?? "" })}
                  {m.ok === undefined
                    ? t("chat.tool_running")
                    : m.ok
                      ? t("chat.tool_ok")
                      : t("chat.tool_failed")}
                </summary>
                {m.toolArgs ? <pre className="kv">args: {m.toolArgs}</pre> : null}
                {m.toolResult !== undefined ? (
                  <pre className="kv">{m.toolResult}</pre>
                ) : null}
              </details>
            ) : (
              <div key={m.id} className={`msg ${m.role}`}>
                {m.text}
              </div>
            ),
          )}

          {store.busy && !store.streaming ? (
            <div className="msg assistant muted">{t("chat.thinking")}</div>
          ) : null}

          {/* Live streamed assistant text; replaced by the final content. */}
          {store.streaming ? (
            <div
              className={`msg assistant${store.streamingActive ? " streaming" : ""}`}
            >
              {store.streaming}
            </div>
          ) : null}
        </div>

        {showJump ? (
          <button className="ghost tiny jump-latest" onClick={jumpToLatest}>
            {t("chat.new_messages")}
          </button>
        ) : null}
      </div>

      <div className="chat-input">
        <textarea
          value={input}
          placeholder={t("chat.input_placeholder")}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void submit();
            }
          }}
        />
        <button disabled={!canSend} onClick={() => void submit()}>
          {store.busy ? t("chat.running") : t("chat.send")}
        </button>
      </div>
    </section>
  );
}
