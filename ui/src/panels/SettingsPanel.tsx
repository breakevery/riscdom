import { useCallback, useEffect, useState } from "react";
import * as api from "../api/tauri";
import type { AppStore } from "../state/appStore";

const BANNER_TEXT: Record<string, string> = {
  no_config: "尚未配置模型。选择服务商并填写 API Key，或使用本地模型。",
  missing_api_key: "缺少 API Key。请填写，或切换到本地模型预设。",
  invalid_base_url: "Base URL 无效，请检查。",
  invalid_config: "配置无效，请检查服务商与 Model。",
};

// Only a non-sensitive UI preference is kept in localStorage.
const REMEMBER_KEY = "riscdom.rememberKey";

function readRemember(): boolean {
  try {
    return localStorage.getItem(REMEMBER_KEY) !== "false";
  } catch {
    return true;
  }
}

function writeRemember(value: boolean): void {
  try {
    localStorage.setItem(REMEMBER_KEY, value ? "true" : "false");
  } catch {
    /* ignore */
  }
}

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
  const [remember, setRemember] = useState<boolean>(readRemember);

  const [readiness, setReadiness] = useState<api.LlmReadiness | null>(null);
  const [probing, setProbing] = useState(false);
  const [probeNote, setProbeNote] = useState<string | null>(null);
  const [suggestion, setSuggestion] = useState<api.LocalProviderInfo | null>(null);
  const [storedPrompt, setStoredPrompt] = useState<string | null>(null);

  const refreshReadiness = useCallback(async () => {
    try {
      setReadiness(await api.getLlmReadiness());
    } catch {
      setReadiness(null);
    }
  }, []);

  const refreshStatus = store.refreshLlmStatus;

  useEffect(() => {
    api
      .getProviderPresets()
      .then(setPresets)
      .catch(() => setNote("加载服务商失败"));

    void (async () => {
      await refreshReadiness();
      // Startup restore: if nothing is configured in memory but a key exists in
      // the OS keyring, load it silently.
      try {
        const status = await api.getLlmConfigStatus();
        if (!status.configured) {
          const pid = status.provider_id || "deepseek";
          if (await api.hasStoredKey(pid)) {
            await api.loadStoredKey(pid);
            setProviderId(pid);
            await refreshStatus();
            await refreshReadiness();
            setNote("已从系统钥匙串恢复 Key");
          }
        } else {
          setProviderId(status.provider_id);
        }
      } catch {
        /* silent */
      }
    })();
  }, [refreshReadiness, refreshStatus]);

  const selected = presets.find((p) => p.id === providerId);
  const isCustom = providerId === "custom";
  const keyNotNeeded = selected ? !selected.requires_key : false;

  const presetName = (id: string) =>
    presets.find((p) => p.id === id)?.display_name ?? id;

  const onProviderChange = (id: string) => {
    setProviderId(id);
    setStoredPrompt(null);
    const preset = presets.find((p) => p.id === id);
    if (preset && preset.id !== "custom") {
      setBaseUrl(preset.base_url);
      setModel(preset.default_model);
    }
    void (async () => {
      try {
        if (await api.hasStoredKey(id)) setStoredPrompt(id);
      } catch {
        /* silent */
      }
    })();
  };

  const loadStored = async (id: string) => {
    try {
      await api.loadStoredKey(id);
      setStoredPrompt(null);
      setNote("已从系统钥匙串加载 Key");
      await refreshStatus();
      await refreshReadiness();
    } catch (e) {
      setNote(String(e));
    }
  };

  const save = async () => {
    setFieldError(null);
    try {
      await api.setLlmConfig(apiKey, baseUrl, model, providerId, remember);
      setApiKey(""); // never keep the key in component memory
      setNote(
        remember ? "已保存到本次会话并写入系统钥匙串" : "已保存到本次会话（仅内存）",
      );
      await refreshStatus();
      await refreshReadiness();
    } catch (e) {
      setFieldError(splitCode(String(e)));
      setNote(null);
    }
  };

  const clear = async () => {
    try {
      await api.clearLlmConfig();
      setNote("已清除（含系统钥匙串条目）");
      setFieldError(null);
      await refreshStatus();
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
      await api.setLlmConfig("", provider.base_url, localModel, provider.id, false);
      setNote("已切换到本地模型");
      await refreshStatus();
      await refreshReadiness();
    } catch (e) {
      setFieldError(splitCode(String(e)));
    }
  };

  const chain = store.auditStatus?.chain;
  const providerName = presetName(store.llmStatus.provider_id);
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

        {storedPrompt ? (
          <div className="banner info">
            <span>检测到已保存的 {presetName(storedPrompt)} Key，是否加载？</span>
            <button className="ghost tiny" onClick={() => void loadStored(storedPrompt)}>
              加载
            </button>
            <button className="ghost tiny" onClick={() => setStoredPrompt(null)}>
              忽略
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

        <label className="check-row">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => {
              setRemember(e.target.checked);
              writeRemember(e.target.checked);
            }}
          />
          <span>保存到系统钥匙串（推荐）</span>
        </label>
        <div className="muted small">未勾选时，API Key 仅保存在本次会话。</div>

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
            ? `已配置 · ${providerName} · ${
                store.llmStatus.persisted ? "已保存到系统钥匙串" : "仅本次会话"
              }`
            : "未配置（API key 不会落盘）"}
        </div>

        <h3>工具链</h3>
        {store.toolchain && !store.toolchain.found ? (
          <div className="banner warn">
            未找到 RISC-V GCC。请安装 xPack RISC-V GCC（
            https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases
            ），或点“手动指定”填入 riscv64-unknown-elf-gcc.exe 的完整路径。
          </div>
        ) : null}
        <div className="status-line">
          <span className={`dot ${store.toolchain?.found ? "ok" : "bad"}`} />
          {store.toolchain?.found ? (
            <>
              RISC-V GCC · <span className="badge">{store.toolchain.source}</span>
            </>
          ) : (
            "未找到工具链"
          )}
          <button className="ghost tiny" onClick={() => void store.refreshToolchain()}>
            重新探测
          </button>
          <button
            className="ghost tiny"
            onClick={() => {
              const p = window.prompt(
                "riscv64-unknown-elf-gcc.exe 的完整路径",
                store.toolchain?.path ?? "",
              );
              if (p) void store.setToolchain(p.trim());
            }}
          >
            手动指定
          </button>
          {store.toolchain?.path ? (
            <button className="ghost tiny" onClick={() => void store.clearToolchain()}>
              清除手动路径
            </button>
          ) : null}
        </div>
        <div className="muted small">{store.toolchain?.path ?? "（未解析到路径）"}</div>
        <details>
          <summary className="muted small">探测详情</summary>
          <pre className="muted small">{store.toolchain?.diagnostics ?? ""}</pre>
        </details>

        <h3>快照</h3>
        <div className="muted small">
          {store.snapshots.length} 个快照
          <button className="ghost tiny" onClick={() => void store.refreshSnapshots()}>
            刷新
          </button>
          <button
            className="ghost tiny"
            disabled={!store.vmIsRunning}
            title={
              store.vmIsRunning ? "保存当前 VM 状态" : "需要运行中的 VM（先让 AI 启动一台）"
            }
            onClick={() => {
              const name = window.prompt("快照名称（字母/数字/-/_）", "snap1");
              if (name) void store.saveSnapshot(name.trim());
            }}
          >
            保存当前状态
          </button>
        </div>
        <div className="muted small">
          VM {store.vmIsRunning ? "运行中（host 持有，跨 run 复用）" : "未运行"}
          · 保存/恢复为真实快照（tcp-relay），需由 host 持有 VM。
        </div>
        <ul className="audit-list">
          {store.snapshots.map((s) => (
            <li key={s.name}>
              <span className="badge">
                {s.mode === "tcp-relay" ? "真实" : "重启式"}
              </span>
              <span className="action">{s.name}</span>
              <span className="muted small">{(s.size_bytes / 1024).toFixed(0)} KB</span>
              <span className="spacer" />
              {s.mode === "tcp-relay" ? (
                <button
                  className="ghost tiny"
                  onClick={() => {
                    if (window.confirm(`从快照“${s.name}”恢复？当前 VM 会被停止。`)) {
                      void store.resumeSnapshot(s.name);
                    }
                  }}
                >
                  恢复
                </button>
              ) : null}
              <button
                className="ghost tiny"
                onClick={() => {
                  if (window.confirm(`删除快照“${s.name}”？`)) {
                    void store.deleteSnapshot(s.name);
                  }
                }}
              >
                删除
              </button>
            </li>
          ))}
        </ul>

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
