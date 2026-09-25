import { useCallback, useEffect, useState } from "react";
import type { AppStore } from "../state/appStore";
import type { NetworkSettings, TokenCheck } from "../api";
import * as api from "../api";
import { t } from "../i18n/index.ts";

/**
 * The network face (v0.9.9 内网接入).
 *
 * Two directions on one screen: **out** — this desktop connects to an in-network
 * RiscDom server; **in** — this desktop serves its own board to the network.
 *
 * Three rules this page keeps:
 *
 * - **the address is a setting, the token is not** (v0.9.9 `"out"`): `remote_url`
 *   goes to `settings.json` like every other preference, and the token goes to the
 *   OS keyring under its address (`save_remote_token`). Nothing this page writes
 *   puts a credential in a file anyone can read.
 * - **the server's token is shown only when asked for** — and that is the *local*
 *   node's own token, the one a phone has to present. The remote one is never read
 *   back into this page: the box below starts empty on purpose, and an empty box
 *   means "keep what is filed".
 * - **connecting is a startup decision**: this page records the choice and asks the
 *   server whether the pair is accepted; the switch itself happens when the app
 *   starts again, which is why the answer says so.
 *
 * The page is the desktop's alone — the browser is not offered the tab — but its
 * four commands are deliberately **local in every mode**: a desktop looking at
 * another machine's node must still be able to see its own board, keep its own
 * token, and get back. That is `api/index.ts`'s "local in every mode" group.
 */

const EMPTY: NetworkSettings = {
  remote_url: null,
  lan_enabled: false,
  lan_bind: null,
  lan_allow_lan: false,
};

/** The same default `riscdom-server` uses when nothing says otherwise. */
const DEFAULT_BIND = "127.0.0.1:7821";

/** The four outcomes of a token check, in the reader's language. */
function checkSentence(found: TokenCheck): string {
  switch (found.kind) {
    case "ok":
      return t("network.check_ok", { version: found.version });
    case "unauthorized":
      return t("network.check_unauthorized");
    case "unreachable":
      return t("network.check_unreachable");
    default:
      return t("network.check_other", { status: found.status });
  }
}

export default function NetworkTab({ store }: { store: AppStore }) {
  const [form, setForm] = useState<NetworkSettings>(() => ({
    ...EMPTY,
    ...(store.network ?? {}),
  }));
  const [saved, setSaved] = useState(false);
  // The token box. Never prefilled — the value lives in the keyring, and an empty
  // box means "leave it alone".
  const [token, setToken] = useState("");
  const [tokenStored, setTokenStored] = useState(false);
  const [busy, setBusy] = useState(false);
  const [found, setFound] = useState<TokenCheck | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  // The local node's own token, shown only when asked for (batch 3's behaviour,
  // unchanged): it is not fetched on mount, not cached, and not stored.
  const [lanToken, setLanToken] = useState<string | null>(null);
  const [tokenProblem, setTokenProblem] = useState<string | null>(null);

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

  const url = (form.remote_url ?? "").trim();

  // Is a token filed for the address on screen? Asked once per address, and the
  // answer is a yes/no — the value itself is not brought back to be rendered.
  useEffect(() => {
    if (url === "") {
      setTokenStored(false);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const stored = await store.readRemoteToken(url);
        if (!cancelled) setTokenStored(stored !== null && stored !== "");
      } catch {
        if (!cancelled) setTokenStored(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [store, url]);

  const edit = (next: Partial<NetworkSettings>) => {
    setSaved(false);
    setNote(null);
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

  /**
   * Record the address, file the token, and ask the server whether the pair is
   * accepted — without retargeting this window at it.
   *
   * The check is the one the login page makes (`verifyToken`), aimed at the
   * address in the box rather than the one in force, so a wrong pair is reported
   * here, now, instead of becoming a window that starts into nothing.
   */
  const connect = async () => {
    setBusy(true);
    setProblem(null);
    setFound(null);
    setNote(null);
    try {
      if (url === "") {
        setProblem(t("network.out_hint"));
        return;
      }
      const typed = token.trim();
      await store.setNetwork({ remote_url: url });
      if (typed !== "") {
        await store.saveRemoteToken(url, typed);
        setTokenStored(true);
      }
      const candidate = typed !== "" ? typed : ((await store.readRemoteToken(url)) ?? "");
      setFound(await api.verifyToken(candidate, url));
      setNote(t("network.restart_needed"));
      setToken("");
    } catch (e) {
      setProblem(String(e));
    } finally {
      setBusy(false);
    }
  };

  /** Forget this server and go back to the embedded host: same path as the gate's. */
  const disconnect = async () => {
    setBusy(true);
    setProblem(null);
    setFound(null);
    setNote(null);
    try {
      if (url !== "") await store.clearRemoteToken(url);
      await store.setNetwork({ remote_url: null });
      await store.restartApp();
    } catch (e) {
      setProblem(String(e));
      setBusy(false);
    }
  };

  const showToken = useCallback(async () => {
    setTokenProblem(null);
    setBusy(true);
    try {
      setLanToken(await store.readLanToken());
    } catch (e) {
      setLanToken(null);
      // The host's own sentence, verbatim: it names the file and how the token
      // comes to exist, which is more useful than anything this page could say.
      setTokenProblem(String(e));
    } finally {
      setBusy(false);
    }
  }, [store]);

  const copyToken = () => {
    if (lanToken === null) return;
    // The async clipboard wants a secure context, which a desktop webview is not
    // guaranteed to be. The value stays selectable either way, so a refusal here
    // is not a dead end and is not worth a sentence of its own.
    void navigator.clipboard?.writeText(lanToken).catch(() => {});
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
            value={token}
            autoComplete="off"
            onChange={(e) => setToken(e.target.value)}
          />
        </label>
        {tokenStored ? (
          <div className="muted small">{t("network.token_saved")}</div>
        ) : null}
        <div>
          <button className="primary" onClick={() => void connect()} disabled={busy}>
            {t("network.connect")}
          </button>{" "}
          <button className="ghost" onClick={() => void disconnect()} disabled={busy}>
            {t("network.disconnect")}
          </button>
        </div>
        {found === null ? null : (
          <div className={found.kind === "ok" ? "muted small" : "field-error"}>
            {checkSentence(found)}
          </div>
        )}
        {note === null ? null : <div className="muted small">{note}</div>}
        {problem === null ? null : <div className="field-error">{problem}</div>}
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
          {lanToken === null ? null : (
            <>
              {" "}
              <button className="ghost" onClick={copyToken}>
                {t("network.token_copy")}
              </button>{" "}
              <code>{lanToken}</code>
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
