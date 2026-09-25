import { useState } from "react";
import * as api from "./api";
import { t } from "./i18n/index.ts";

/**
 * The Web client's front door (v0.9 D2b-2).
 *
 * The desktop never shows this: its host is in the same process, so there is no
 * token to type and no way to be unauthenticated. This is a **gate, not a route** —
 * `App` renders it *instead of* the shell until a token is accepted, which is also
 * what keeps the store from mounting (and firing a dozen unauthenticated requests)
 * before there is one.
 *
 * The token is checked with one `GET /v0/health` **before** it is installed, so a
 * rejected attempt cannot leave a bad token behind for the next call. Where it is
 * kept after that is `api`'s business (`sessionStorage`, or `localStorage` when the
 * box below is ticked) — never the URL.
 */
export default function Login({
  onLoggedIn,
  onUseLocal,
  useLocalBusy = false,
  useLocalProblem = null,
}: {
  onLoggedIn: () => void;
  /**
   * Leave a remote server and come back as the embedded host (v0.9.9 `"out"`).
   *
   * Offered **only by a Tauri runtime**, because it is the one control on this page
   * that acts on the machine the page is running on rather than on the server it is
   * failing to reach. In a browser it is `undefined` and nothing is rendered.
   */
  onUseLocal?: () => void;
  useLocalBusy?: boolean;
  useLocalProblem?: string | null;
}) {
  const [candidate, setCandidate] = useState("");
  const [remember, setRemember] = useState(false);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<api.TokenCheck | null>(null);

  const submit = async () => {
    const token = candidate.trim();
    if (token === "" || busy) return;
    setBusy(true);
    setProblem(null);
    const check = await api.verifyToken(token);
    setBusy(false);
    if (check.kind === "ok") {
      api.setToken(token, remember);
      onLoggedIn();
      return;
    }
    setProblem(check);
  };

  return (
    <div
      className="login-page"
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        minHeight: "100vh",
        padding: 16,
      }}
    >
      <section className="panel" style={{ width: "min(420px, 92vw)" }}>
        <header className="panel-head">{t("login.heading")}</header>

        <label>
          {t("login.token_label")}
          <input
            type="password"
            value={candidate}
            autoComplete="off"
            autoFocus
            onChange={(e) => setCandidate(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
          />
        </label>
        <div className="muted small">{t("login.token_hint")}</div>

        <label className="check-row">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          <span>{t("login.remember")}</span>
        </label>

        {problem === null ? null : (
          <div className="field-error">
            {problem.kind === "unauthorized"
              ? t("login.unauthorized")
              : problem.kind === "unreachable"
                ? t("login.unreachable")
                : t("login.other")}
          </div>
        )}

        <button
          className="primary"
          disabled={busy || candidate.trim() === ""}
          onClick={() => void submit()}
        >
          {t("login.submit")}
        </button>

        {onUseLocal === undefined ? null : (
          <>
            <div className="muted small">{t("login.use_local_hint")}</div>
            <button
              className="ghost"
              disabled={busy || useLocalBusy}
              onClick={onUseLocal}
            >
              {t("login.use_local")}
            </button>
          </>
        )}
        {useLocalProblem === null ? null : (
          <div className="field-error">{useLocalProblem}</div>
        )}
      </section>
    </div>
  );
}
