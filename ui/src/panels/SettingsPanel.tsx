import { useState } from "react";
import * as api from "../api/tauri";
import type { AppStore } from "../state/appStore";

export default function SettingsPanel({ store }: { store: AppStore }) {
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.deepseek.com");
  const [model, setModel] = useState("deepseek-chat");
  const [note, setNote] = useState<string | null>(null);

  const save = async () => {
    try {
      await api.setLlmConfig(apiKey, baseUrl, model);
      setApiKey(""); // never keep the key in component memory
      setNote("已保存到本次会话（仅内存）");
      await store.refreshLlmStatus();
    } catch (e) {
      setNote(`保存失败：${String(e)}`);
    }
  };

  const clear = async () => {
    try {
      await api.clearLlmConfig();
      setNote("已清除");
      await store.refreshLlmStatus();
    } catch (e) {
      setNote(String(e));
    }
  };

  const chain = store.auditStatus?.chain;

  return (
    <section className="panel">
      <header className="panel-head">设置</header>

      <div className="settings-body">
        <h3>LLM 配置</h3>
        <label>
          API Key
          <input
            type="password"
            value={apiKey}
            autoComplete="off"
            placeholder="sk-…（仅保存在内存）"
            onChange={(e) => setApiKey(e.target.value)}
          />
        </label>
        <label>
          Base URL
          <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
        </label>
        <label>
          Model
          <input value={model} onChange={(e) => setModel(e.target.value)} />
        </label>
        <div className="row">
          <button onClick={() => void save()}>保存到本次会话</button>
          <button className="ghost" onClick={() => void clear()}>
            清除
          </button>
        </div>
        {note ? <div className="muted small">{note}</div> : null}

        <div className="status-line">
          <span className={`dot ${store.llmStatus.configured ? "ok" : "off"}`} />
          {store.llmStatus.configured
            ? `已配置 · ${store.llmStatus.base_url} · ${store.llmStatus.model}`
            : "未配置（API key 不会落盘）"}
        </div>

        <h3>工作区</h3>
        <div className="muted small">
          {store.workspaceFiles.length} 个文件
          <button className="ghost tiny" onClick={() => void store.refreshWorkspace()}>
            刷新
          </button>
        </div>
        <ul className="file-list">
          {store.workspaceFiles.slice(0, 12).map((f) => (
            <li key={f}>{f}</li>
          ))}
        </ul>

        <h3>审计</h3>
        <div className="status-line">
          <span className={`dot ${chain?.status === "Intact" ? "ok" : chain ? "bad" : "off"}`} />
          {store.auditStatus
            ? `${store.auditStatus.count} 条事件 · ${
                chain?.status === "Intact"
                  ? `链完整 (${chain.length})`
                  : chain
                    ? `链断裂 @${chain.at_id}`
                    : "未知"
              }`
            : "加载中…"}
          <button className="ghost tiny" onClick={() => void store.refreshAudit()}>
            刷新
          </button>
        </div>

        <label className="row">
          <span className="small">按 actor 过滤</span>
          <input
            value={store.auditActorFilter}
            placeholder="sandbox / agent / host / human"
            onChange={(e) => store.setAuditActorFilter(e.target.value)}
            onBlur={() => void store.refreshAudit()}
          />
        </label>

        <ul className="audit-list">
          {store.auditEvents.map((e) => (
            <li key={e.id}>
              <span className="badge">{e.actor}</span>
              <span className="action">{e.action}</span>
              <span className="muted small">#{e.id}</span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
