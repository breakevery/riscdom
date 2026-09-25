import { useCallback, useEffect, useState } from "react";
import type { AppStore } from "../state/appStore";
import type { NetworkSettings } from "../api";
import { t } from "../i18n/index.ts";

/**
 * The network face (v0.9.9 内网接入).
 *
 * **Configuration only.** This page writes settings and reads the token a started
 * server would demand; it deliberately starts nothing — the batch that binds the
 * socket is the next one, and the "out" group is a placeholder for the one after
 * that. Its button is disabled and says when it will work, rather than pretending
 * to connect.
 *
 * The token is shown **only when asked for** and lives in this component's state:
 * it is not fetched on mount, not cached anywhere else, and not stored.
 */

const EMPTY: NetworkSettings = {
  remote_url: null,
  remote_token: null,
  lan_enabled: false,
  lan_bind: null,
  lan_allow_lan: false,
};

/** The same default `riscdom-server` uses when nothing says otherwise. */
const DEFAULT_BIND = "127.0.0.1:7821";

export default function NetworkTab({ store }: { store: AppStore }) {
  const [form, setForm] = useState<NetworkSettings>(() => ({
    ...EMPTY,
    ...(store.network ?? {}),
  }));
  const [saved, setSaved] = useState(false);
  const [token, setToken] = useState<string | null>(null);
  const [tokenProblem, setTokenProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Read once, on mount: the page shows what the node is wired to do. The answer
  // is a command round trip, so it lands **after** the first render — which is
  // why the effect below has to copy it into the form. Without that, a node that
  // already has its board switched on would show every control off until somebody
  // edited and saved it again (found by walking this page, v0.9.9 batch 2).
  useEffect(() => {
    void store.refreshNetwork();
    void store.refreshLan();
  }, [store]);

  // The form follows the store's answer. It cannot clobber typing: `network` only
  // changes on that mount read and on a save, and a save carries exactly what the
  // form already holds.
  useEffect(() => {
    if (store.network !== null) setForm({ ...EMPTY, ...store.network });
  }, [store.network]);

  const edit = (next: Partial<NetworkSettings>) => {
    setSaved(false);
    setForm((prev) => ({ ...prev, ...next }));
  };

  const save = async () => {
    setBusy(true);
    await store.setNetwork(form);
    // The board follows the settings, so the state it reports is read again
    // rather than guessed: a port already in use is answered here.
    await store.refreshLan();
    setBusy(false);
    setSaved(true);
  };

  const showToken = useCallback(async () => {
    setTokenProblem(null);
    setBusy(true);
    try {
      setToken(await store.readLanToken());
    } catch (e) {
      setToken(null);
      // The host's own sentence, verbatim: it names the file and how the token
      // comes to exist, which is more useful than anything this page could say.
      setTokenProblem(String(e));
    } finally {
      setBusy(false);
    }
  }, [store]);

  const copyToken = () => {
    if (token === null) return;
    // The async clipboard wants a secure context, which a desktop webview is not
    // guaranteed to be. The value stays selectable either way, so a refusal here
    // is not a dead end and is not worth a sentence of its own.
    void navigator.clipboard?.writeText(token).catch(() => {});
  };

  const bind = form.lan_bind ?? DEFAULT_BIND;
  const port = bind.includes(":") ? bind.split(":").pop() : bind;
  const lan = store.lan;

  return (
    <>
      <div className="settings-section">
        <h3>{t("network.out_heading")}</h3>
        <p className="muted small">{t("network.out_hint")}</p>
        <label>
          {t("network.remote_url")}
          <input
            type="text"
            value={form.remote_url ?? ""}
            placeholder="http://192.168.1.10:7821"
            onChange={(e) =>
              edit({ remote_url: e.target.value === "" ? null : e.target.value })
            }
          />
        </label>
        <label>
          {t("network.remote_token")}
          <input
            type="password"
            value={form.remote_token ?? ""}
            autoComplete="off"
            onChange={(e) =>
              edit({ remote_token: e.target.value === "" ? null : e.target.value })
            }
          />
        </label>
        <button className="primary" disabled title={t("network.out_hint")}>
          {t("network.connect")}
        </button>
      </div>

      <div className="settings-section">
        <h3>{t("network.in_heading")}</h3>
        <label className="check-row">
          <input
            type="checkbox"
            checked={form.lan_enabled}
            onChange={(e) => edit({ lan_enabled: e.target.checked })}
          />
          <span>{t("network.lan_enabled")}</span>
        </label>
        <label>
          {t("network.lan_bind")}
          <input
            type="text"
            value={form.lan_bind ?? ""}
            placeholder={DEFAULT_BIND}
            onChange={(e) =>
              edit({ lan_bind: e.target.value === "" ? null : e.target.value })
            }
          />
        </label>
        <div className="muted small">{t("network.lan_bind_hint")}</div>
        <label className="check-row">
          <input
            type="checkbox"
            checked={form.lan_allow_lan}
            onChange={(e) => edit({ lan_allow_lan: e.target.checked })}
          />
          <span>{t("network.allow_lan")}</span>
        </label>
        {form.lan_allow_lan ? (
          <div className="field-error" role="alert">
            {t("network.allow_lan_warning")}
          </div>
        ) : null}
        <div className="muted small">
          {t("network.lan_url")}:{" "}
          {lan?.running ? (
            <code>{`http://${lan.address ?? "<this machine>"}:${port}`}</code>
          ) : (
            <span>{t("network.lan_url_pending")}</span>
          )}{" "}
          <span className={lan?.running ? "ok" : "muted"}>
            {lan?.running ? t("network.lan_state_running") : t("network.lan_url_pending")}
          </span>
          {lan?.bound ? <span className="muted"> ({lan.bound})</span> : null}
        </div>
        <div className="muted small">{t("network.firewall_hint")}</div>
        {lan?.problem ? <div className="field-error">{lan.problem}</div> : null}
        <div className="muted small">{t("network.token_path")}</div>
        <div>
          <button className="ghost" onClick={() => void showToken()} disabled={busy}>
            {t("network.token_show")}
          </button>
          {token === null ? null : (
            <>
              {" "}
              <button className="ghost" onClick={copyToken}>
                {t("network.token_copy")}
              </button>{" "}
              <code>{token}</code>
            </>
          )}
        </div>
        {tokenProblem === null ? null : (
          <div className="field-error">{tokenProblem}</div>
        )}
        <div>
          <button className="primary" onClick={() => void save()} disabled={busy}>
            {t("network.save")}
          </button>
          {saved ? <span className="muted small"> {t("network.saved")}</span> : null}
        </div>
      </div>
    </>
  );
}
