import { useState } from "react";
import * as api from "../api/tauri";
import type { AppStore } from "../state/appStore";

export default function ChatPanel({ store }: { store: AppStore }) {
  const [input, setInput] = useState("");
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
      <header className="panel-head">对话框</header>

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
