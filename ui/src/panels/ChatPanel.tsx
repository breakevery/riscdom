import { useState } from "react";
import * as api from "../api/tauri";
import { relTime, type AppStore } from "../state/appStore";

export default function ChatPanel({ store }: { store: AppStore }) {
  const [input, setInput] = useState("");
  const [showSessions, setShowSessions] = useState(false);
  const canSend = !store.busy && input.trim().length > 0;

  const submit = async () => {
    if (!canSend) return;
    const text = input;
    setInput("");

    // No-key fallback: check readiness before calling the backend.
    try {
      const readiness = await api.getLlmReadiness();
      if (!readiness.ready) {
        store.pushSystem(
          readiness.suggestion ?? "模型未就绪，请在设置中配置服务商与 API Key。",
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
        <span>对话框</span>
        <button className="ghost tiny" onClick={() => setShowSessions((v) => !v)}>
          会话 ({store.sessions.length})
        </button>
      </header>

      {store.toolchainMissing ? (
        <div className="banner warn">
          未找到 RISC-V GCC：现在无法编译。请到「设置 → 工具链」点“重新探测”或“手动指定”填入
          riscv64-unknown-elf-gcc.exe 的完整路径（详见 docs/toolchain-setup.md）。
        </div>
      ) : null}

      {showSessions ? (
        <div className="session-list">
          <div className="row">
            <button className="ghost tiny" onClick={() => void store.newSession()}>
              新建会话
            </button>
            <button className="ghost tiny" onClick={() => void store.refreshSessions()}>
              刷新
            </button>
          </div>
          {store.sessions.length === 0 ? (
            <div className="muted small">（暂无会话）</div>
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
                    const next = window.prompt("重命名会话", s.title);
                    if (next && next.trim()) void store.renameSession(s.id, next.trim());
                  }}
                >
                  改名
                </button>
                <button
                  className="ghost tiny"
                  onClick={() => {
                    if (window.confirm(`删除会话“${s.title}”？此操作不可撤销。`)) {
                      void store.deleteSession(s.id);
                    }
                  }}
                >
                  删除
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="chat-log">
        {store.messages.length === 0 ? (
          <div className="muted small">
            用自然语言描述你想让 AI 在沙箱里做什么，例如：
            “写一个 RISC-V 裸机 Hello World，编译运行并把串口输出读回来”。
          </div>
        ) : null}

        {store.messages.map((m) =>
          m.role === "tool" ? (
            <details key={m.id} className="msg tool">
              <summary>
                🔧 {m.toolName}
                {m.ok === undefined ? " (running…)" : m.ok ? " ✓" : " ✗"}
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
          <div className="msg assistant muted">思考中…</div>
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

      <div className="chat-input">
        <textarea
          value={input}
          placeholder="描述任务…（Enter 发送，Shift+Enter 换行）"
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void submit();
            }
          }}
        />
        <button disabled={!canSend} onClick={() => void submit()}>
          {store.busy ? "运行中…" : "发送"}
        </button>
      </div>
    </section>
  );
}
