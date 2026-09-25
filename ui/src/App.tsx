import { useEffect, useState } from "react";
import AppShell from "./layout/AppShell";
import Login from "./Login";
import * as api from "./api";
import { t } from "./i18n/index.ts";

/**
 * The front door (v0.9 D2b-2; the remote mode is v0.9.9 内网接入 `"out"`).
 *
 * Three answers, and the order they are given in is the whole point:
 *
 * 1. **Is there a mode to settle?** On the desktop only, and before anything else:
 *    the settings are read once, and if they name a server this window becomes a
 *    client of *that* node rather than of its own. It is settled here, outside the
 *    shell, because the mode decides which transport every call takes — a panel
 *    that had already read from one host would be showing that host's data under
 *    the other one's name.
 * 2. **Is this the local host?** If so the shell renders, exactly as it did before
 *    this batch: the desktop's host lives in this very process, so there is no
 *    token to ask for and no endpoint to ask it of. That check has to come
 *    **first** — v0.9.0 shipped without it and the desktop stopped at the login
 *    screen, because `currentToken()` is the Web client's (a desktop has none) and
 *    `verifyToken()` would call a `/v0/health` no desktop process serves (v0.9.1).
 * 3. **Otherwise the gate.** A browser and a desktop that connected to another
 *    server are the same thing from here on: a token opens the shell, anything else
 *    gets the login screen — which is also where the way back to the embedded host
 *    lives (`onUseLocal` below).
 *
 * The gate lives here rather than inside `AppShell` on purpose: `AppShell` is where
 * `useAppStore()` is called, so rendering the shell only once there is a token is
 * what keeps the store from mounting (and firing a dozen unauthenticated requests)
 * before there is one. `api` reads any stored token when it loads, so a reload with
 * a token in `sessionStorage` goes straight through.
 */
export default function App() {
  // A browser has nothing to settle: its mode is decided by what it is.
  const [ready, setReady] = useState(() => !api.isTauriRuntime());

  useEffect(() => {
    if (!api.isTauriRuntime()) return;
    let cancelled = false;
    void (async () => {
      try {
        const net = await api.getNetwork();
        const url = (net?.remote_url ?? "").trim();
        if (url !== "") {
          api.setApiBase(url);
          api.setImpl("remote", url);
          // The address is a setting; the token is in the OS keyring, so it is
          // asked for here and installed for this session only. Nothing is written
          // back to storage: one credential, one home.
          const token = await api.readRemoteToken(url);
          if (token !== null && token !== "") api.setToken(token, false);
        }
      } catch {
        // A settings file that cannot be read is not a reason to refuse to open.
        // The embedded host is what every release before this one used, and it is
        // exactly what `local` mode means.
      } finally {
        if (!cancelled) setReady(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // One round trip, so the gate never flashes over a window that is about to
  // become the shell.
  if (!ready) return <Booting />;

  if (api.isLocalHost()) return <AppShell />;

  return <WebGate />;
}

/** The half-second the mode takes to settle. */
function Booting() {
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
        <header className="panel-head">RiscDom</header>
        <div className="muted small">{t("app.booting")}</div>
      </section>
    </div>
  );
}

/**
 * The gate: a token in hand, or the login screen.
 *
 * A separate component, not a branch inside `App`, so that the hook below is never
 * reached on the local host — no token is read and no login screen is rendered
 * there.
 */
function WebGate() {
  const [authenticated, setAuthenticated] = useState(
    () => api.currentToken() !== "",
  );
  const [leaving, setLeaving] = useState(false);
  const [leaveProblem, setLeaveProblem] = useState<string | null>(null);

  /**
   * Go back to the embedded host.
   *
   * All three steps are **local in every mode** (`api/index.ts`'s exception group),
   * which is the only reason this can work at all: the server this window cannot
   * reach is not the machine that has to act. The address is cleared from the
   * settings, the token is deleted from the keyring, and the app starts again —
   * there is no partial state to leave behind, because a mode is decided once, at
   * startup.
   */
  const useLocal = async () => {
    setLeaving(true);
    setLeaveProblem(null);
    try {
      const url = api.getApiBase();
      if (url !== "") await api.clearRemoteToken(url);
      const net = await api.getNetwork();
      await api.setNetwork({
        remote_url: null,
        lan_enabled: net?.lan_enabled ?? false,
        lan_bind: net?.lan_bind ?? null,
        lan_allow_lan: net?.lan_allow_lan ?? false,
      });
    } catch (e) {
      // A write that failed must not be followed by a restart: it would come back
      // to this same screen, and the next attempt would have no way to know why.
      setLeaveProblem(String(e));
      setLeaving(false);
      return;
    }
    await api.restartApp();
  };

  if (!authenticated) {
    return (
      <Login
        onLoggedIn={() => setAuthenticated(true)}
        onUseLocal={api.isTauriRuntime() ? () => void useLocal() : undefined}
        useLocalBusy={leaving}
        useLocalProblem={leaveProblem}
      />
    );
  }
  return <AppShell />;
}
