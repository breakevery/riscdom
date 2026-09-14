import { useCallback, useEffect, useState } from "react";
import * as api from "../api/tauri";
import type { AppStore } from "../state/appStore";

const BANNER_TEXT: Record<string, string> = {
  no_config: "尚未配置模型。选择服务商并填写 API Key，或使用本地模型。",
  missing_api_key: "缺少 API Key。请填写，或切换到本地模型预设。",
  invalid_base_url: "Base URL 无效，请检查。",
  invalid_config: "配置无效，请检查服务商与 Model。",
};

function splitCode(message: string): { code: string; text: string } {
  const i = message.indexOf("|");
  if (i < 0) return { code: message, text: message };
  return { code: message.slice(0, i), text: message.slice(i + 1) };
}

export default function SettingsPanel({ store }: { store: AppStore }) {
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.deepseek.com");
  const [model, setModel] = useState("deepseek-chat");
  const [providerId, setProviderId] = useState("deepseek");
  const [presets, setPresets] = useState<api.ProviderPreset[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const [fieldError, setFieldError] = useState<{ code: string; text: string } | null>(null);

  const [readiness, setReadiness] = useState<api.LlmReadiness | null>(null);
  const [probing, setProbing] = useState(false);
  const [probeNote, setProbeNote] = useState<string | null>(null);
  const [suggestion, setSuggestion] = useState<api.LocalProviderInfo | null>(null);

  const refreshReadiness = useCallback(async () => {
    try {
      setReadiness(await api.getLlmReadiness());
    } catch {
      setReadiness(null);
    }
  }, []);

  useEffect(() => {
    api
      .getProviderPresets()
      .then(setPresets)
      .catch(() => setNote("加载服务商失败"));
    void refreshReadiness();
  }, [refreshReadiness]);

  const selected = presets.find((p) => p.id === providerId);
  const isCustom = providerId === "custom";
  const keyNotNeeded = selected ? !selected.requires_key : false;

  const onProviderChange = (id: string) => {
    setProviderId(id);
    const preset = presets.find((p) => p.id === id);
    if (preset && preset.id !== "custom") {
      setBaseUrl(preset.base_url);
      setModel(preset.default_model);
    }
  };

  const save = async () => {
    setFieldError(null);
    try {
      await api.setLlmConfig(apiKey, baseUrl, model, providerId);
      setApiKey(""); // never keep the key in component memory
      setNote("已保存到本次会话（仅内存）");
      await store.refreshLlmStatus();
      await refreshReadiness();
    } catch (e) {
      const parsed = splitCode(String(e));
      setFieldError(parsed);
      setNote(null);
    }
  };

  const clear = async () => {
    try {
      await api.clearLlmConfig();
      setNote("已清除");
      setFieldError(null);
      await store.refreshLlmStatus();
      await refreshReadiness();
    } catch (e) {
      setNote(String(e));
    }
  };

  const detectLocal = async () => {
    setProbing(true);
    setProbeNote(null);
    setSuggestion(null);
    try {
      const result = await api.probeLocalLlm();
      if (result.found && result.providers.length > 0) {
        setSuggestion(result.providers[0]);
      } else {
        setProbeNote("未检测到本地模型");
      }
    } catch {
      setProbeNote("未检测到本地模型");
    } finally {
      setProbing(false);
    }
  };

  const useLocal = async (provider: api.LocalProviderInfo) => {
    const preset = presets.find((p) => p.id === provider.id);
    const localModel = provider.models[0] ?? preset?.default_model ?? "";
    setProviderId(provider.id);
    setBaseUrl(provider.base_url);
    setModel(localModel);
    setSuggestion(null);
    try {
      await api.setLlmConfig("", provider.base_url, localModel, provider.id);
      setNote("已切换到本地模型");
      await store.refreshLlmStatus();
      await refreshReadiness();
    } catch (e) {
      setFieldError(splitCode(String(e)));
    }
  };

  const chain = store.auditStatus?.chain;
  const providerName =
    presets.find((p) => p.id === store.llmStatus.provider_id)?.display_name ??
    store.llmStatus.provider_id;
  const bannerReason = readiness && !readiness.ready ? readiness.reason ?? "" : null;

  return (
    <section className="panel">
      <header className="panel-head">设置</header>

      <div className="settings-body">
        {bannerReason !== null ? (
          <div className="banner warn">
            <span>{BANNER_TEXT[bannerReason] ?? "模型未就绪，请检查配置。"}</span>
            <button className="ghost tiny" disabled={probing} onClick={() => void detectLocal()}>
              {probing ? "检测中…" : "检测本地模型"}
            </button>
          </div>
        ) : null}

        {suggestion ? (
          <div className="banner info">
            <span>
              检测到本地模型 {suggestion.display_name}（{suggestion.base_url}，
              {suggestion.models.length} 个模型）。是否使用？
            </span>
            <button className="ghost tiny" onClick={() => void useLocal(suggestion)}>
              使用
            </button>
            <button className="ghost tiny" onClick={() => setSuggestion(null)}>
              忽略
            </button>
          </div>
        ) : null}
        {probeNote ? <div className="muted small">{probeNote}</div> : null}

        <h3>LLM 配置</h3>

        <label>
          服务商
          <select value={providerId} onChange={(e) => onProviderChange(e.target.value)}>
            {presets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.display_name}
              </option>
            ))}
          </select>
        </label>

        <label>
          API Key
          <input
            type="password"
            value={apiKey}
            autoComplete="off"
            disabled={keyNotNeeded}
            placeholder={keyNotNeeded ? "本地模型无需 key" : "sk-…（仅保存在内存）"}
            onChange={(e) => setApiKey(e.target.value)}
          />
        </label>
        {keyNotNeeded ? <div className="muted small">本地模型无需 key</div> : null}
        {fieldError?.code === "missing_api_key" ? (
          <div className="field-error">{fieldError.text}</div>
        ) : null}

        <label>
          Base URL
          <input
            value={baseUrl}
            placeholder={isCustom ? "https://your-endpoint/v1" : ""}
            onChange={(e) => setBaseUrl(e.target.value)}
          />
        </label>
        {fieldError?.code === "invalid_base_url" ? (
          <div className="field-error">{fieldError.text}</div>
        ) : null}

        <label>
          Model
          <input
            value={model}
            placeholder={isCustom ? "your-model" : ""}
            onChange={(e) => setModel(e.target.value)}
          />
        </label>
        {fieldError?.code === "invalid_config" ? (
          <div className="field-error">{fieldError.text}</div>
        ) : null}

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
            ? `已配置 · ${providerName} · ${store.llmStatus.base_url} · ${store.llmStatus.model}`
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
