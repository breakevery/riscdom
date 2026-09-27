import { useCallback, useEffect, useState } from "react";
import * as api from "../api";
import type { AppStore } from "../state/appStore";
import { t } from "../i18n/index.ts";
import type { StringKey } from "../i18n/index.ts";

const BANNER_TEXT: Record<string, StringKey | undefined> = {
  no_config: "model.banner.no_config",
  missing_api_key: "model.banner.missing_api_key",
  invalid_base_url: "model.banner.invalid_base_url",
  invalid_config: "model.banner.invalid_config",
};

// Only a non-sensitive UI preference is kept in localStorage.
const REMEMBER_KEY = "riscdom.rememberKey";

// The form's opening values. They are named because the picker needs them too: an executor
// with nothing configured opens on these rather than keeping the previous executor's values
// (v1.0 M2b-3b).
const DEFAULT_PROVIDER_ID = "deepseek";
const DEFAULT_BASE_URL = "https://api.deepseek.com";
const DEFAULT_MODEL = "deepseek-chat";

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

export default function ModelTab({ store }: { store: AppStore }) {
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState(DEFAULT_BASE_URL);
  const [model, setModel] = useState(DEFAULT_MODEL);
  const [providerId, setProviderId] = useState(DEFAULT_PROVIDER_ID);
  const [presets, setPresets] = useState<api.ProviderPreset[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const [fieldError, setFieldError] = useState<{ code: string; text: string } | null>(null);
  const [remember, setRemember] = useState<boolean>(readRemember);

  const [readiness, setReadiness] = useState<api.LlmReadiness | null>(null);
  const [probing, setProbing] = useState(false);
  const [probeNote, setProbeNote] = useState<string | null>(null);
  const [suggestion, setSuggestion] = useState<api.LocalProviderInfo | null>(null);
  const [storedPrompt, setStoredPrompt] = useState<string | null>(null);

  // Which executor this form configures (v1.0 M2b-3b). `undefined` is this node's own —
  // the empty option, which both transports read as "no executor named".
  const executor = store.executorSelection || undefined;

  const refreshReadiness = useCallback(async () => {
    try {
      setReadiness(await api.getLlmReadiness(executor));
    } catch {
      setReadiness(null);
    }
  }, [executor]);

  // The picker's fleet: this node first, then the labels a task can name.
  const refreshExecutors = store.refreshExecutors;
  useEffect(() => {
    void refreshExecutors();
  }, [refreshExecutors]);

  const refreshStatus = store.refreshLlmStatus;

  useEffect(() => {
    api
      .getProviderPresets()
      .then(setPresets)
      .catch(() => setNote(t("model.presets_failed")));

    void (async () => {
      await refreshReadiness();
      // Startup restore (v0.9) and executor switch (v1.0 M2b-3b). The form shows the
      // **selected** executor's own configuration, and one with nothing configured opens
      // on the provider defaults instead of keeping the previous executor's values. The
      // key box is always emptied: a key typed for one executor is not the next one's.
      setApiKey("");
      setStoredPrompt(null);
      try {
        const status = await api.getLlmConfigStatus(executor);
        if (!status.configured) {
          setProviderId(status.provider_id);
          setBaseUrl(DEFAULT_BASE_URL);
          setModel(DEFAULT_MODEL);
          // If nothing is configured in memory but a key exists in the OS keyring,
          // load it silently — and show what it loaded.
          const pid = status.provider_id || DEFAULT_PROVIDER_ID;
          if (await api.hasStoredKey(pid, executor)) {
            await api.loadStoredKey(pid, executor);
            await refreshStatus();
            await refreshReadiness();
            const loaded = await api.getLlmConfigStatus(executor);
            if (loaded.configured) {
              setProviderId(loaded.provider_id);
              setBaseUrl(loaded.base_url);
              setModel(loaded.model);
            }
            setNote(t("model.key_restored"));
          }
        } else {
          setProviderId(status.provider_id);
          setBaseUrl(status.base_url);
          setModel(status.model);
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
        if (await api.hasStoredKey(id, executor)) setStoredPrompt(id);
      } catch {
        /* silent */
      }
    })();
  };

  const loadStored = async (id: string) => {
    try {
      await api.loadStoredKey(id, executor);
      setStoredPrompt(null);
      setNote(t("model.key_loaded"));
      await refreshStatus();
      await refreshReadiness();
    } catch (e) {
      setNote(String(e));
    }
  };

  const save = async () => {
    setFieldError(null);
    try {
      await api.setLlmConfig(apiKey, baseUrl, model, providerId, remember, executor);
      setApiKey(""); // never keep the key in component memory
      setNote(
        remember ? t("model.saved_keyring") : t("model.saved_memory"),
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
      await api.clearLlmConfig(executor);
      setNote(t("model.cleared"));
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
        setProbeNote(t("model.no_local"));
      }
    } catch {
      setProbeNote(t("model.no_local"));
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
      await api.setLlmConfig("", provider.base_url, localModel, provider.id, false, executor);
      setNote(t("model.switched_local"));
      await refreshStatus();
      await refreshReadiness();
    } catch (e) {
      setFieldError(splitCode(String(e)));
    }
  };

  const providerName = presetName(store.llmStatus.provider_id);
  const bannerReason = readiness && !readiness.ready ? readiness.reason ?? "" : null;

  return (
    <>
      {bannerReason !== null ? (
        <div className="banner warn">
          <span>
            {BANNER_TEXT[bannerReason] !== undefined
              ? t(BANNER_TEXT[bannerReason])
              : t("model.not_ready")}
          </span>
          <button className="ghost tiny" disabled={probing} onClick={() => void detectLocal()}>
            {probing ? t("model.detecting") : t("model.detect_local")}
          </button>
        </div>
      ) : null}

      {storedPrompt ? (
        <div className="banner info">
          <span>{t("model.stored_prompt", { provider: presetName(storedPrompt) })}</span>
          <button className="ghost tiny" onClick={() => void loadStored(storedPrompt)}>
            {t("model.load")}
          </button>
          <button className="ghost tiny" onClick={() => setStoredPrompt(null)}>
            {t("model.ignore")}
          </button>
        </div>
      ) : null}

      {suggestion ? (
        <div className="banner info">
          <span>
            {t("model.local_found", {
              name: suggestion.display_name,
              url: suggestion.base_url,
              count: suggestion.models.length,
            })}
          </span>
          <button className="ghost tiny" onClick={() => void useLocal(suggestion)}>
            {t("model.use")}
          </button>
          <button className="ghost tiny" onClick={() => setSuggestion(null)}>
            {t("model.ignore")}
          </button>
        </div>
      ) : null}
      {probeNote ? <div className="muted small">{probeNote}</div> : null}

      <label>
        {t("model.executor")}
        <select
          value={store.executorSelection}
          title={t("model.executor_hint")}
          onChange={(e) => store.setExecutorSelection(e.target.value)}
        >
          <option value="" title={t("model.executor_this_node")}>
            {t("model.executor_this_node")}
          </option>
          {store.executors.map((executorOption) => (
            <option key={executorOption.agent_id} value={executorOption.agent_id}>
              {executorOption.agent_id}
            </option>
          ))}
        </select>
      </label>

      <label>
        {t("model.provider")}
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
          placeholder={keyNotNeeded ? t("model.no_key_needed") : t("model.key_placeholder")}
          onChange={(e) => setApiKey(e.target.value)}
        />
      </label>
      {keyNotNeeded ? (
        <div className="muted small">{t("model.no_key_needed")}</div>
      ) : null}

      <label className="check-row">
        <input
          type="checkbox"
          checked={remember}
          onChange={(e) => {
            setRemember(e.target.checked);
            writeRemember(e.target.checked);
          }}
        />
        <span>{t("model.remember")}</span>
      </label>
      <div className="muted small">{t("model.remember_hint")}</div>

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
        <button onClick={() => void save()}>{t("model.save")}</button>
        <button className="ghost" onClick={() => void clear()}>
          {t("model.clear")}
        </button>
      </div>
      {note ? <div className="muted small">{note}</div> : null}

      <div className="status-line">
        <span className={`dot ${store.llmStatus.configured ? "ok" : "off"}`} />
        {store.llmStatus.configured
          ? t("model.status_configured", {
              provider: providerName,
              state: store.llmStatus.persisted
                ? t("model.status_persisted")
                : t("model.status_session_only"),
            })
          : t("model.status_unconfigured")}
      </div>
    </>
  );
}
