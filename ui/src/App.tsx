import { useState } from "react";
import AppShell from "./layout/AppShell";
import Login from "./Login";
import * as api from "./api";

/**
 * The front door (v0.9 D2b-2).
 *
 * On the **desktop** this is a one-liner: the shell's host lives in this very
 * process, so there is no token to ask for and no endpoint to ask it of. That check
 * has to come **first** — v0.9.0 shipped without it and the desktop stopped at the
 * login screen, because `currentToken()` is the Web client's (a desktop has none)
 * and `verifyToken()` would call a `/v0/health` no desktop process serves (v0.9.1).
 *
 * On the **Web** it is the gate — a stored token opens the shell, and anything else
 * gets the login screen.
 *
 * The gate lives here rather than inside `AppShell` on purpose: `AppShell` is where
 * `useAppStore()` is called, so rendering the shell only once there is a token is
 * what keeps the store from mounting (and firing a dozen unauthenticated requests)
 * before there is one. `api` reads any stored token when it loads, so a reload with
 * a token in `sessionStorage` goes straight through.
 */
export default function App() {
  // The desktop is never gated, and it is answered before any token is consulted.
  if (api.isTauriRuntime()) return <AppShell />;

  return <WebGate />;
}

/**
 * The Web client's gate: a token in hand, or the login screen.
 *
 * A separate component, not a branch inside `App`, so that the hook below is never
 * reached on the desktop — no token is read, no login screen is rendered and no
 * `/v0/health` is asked for there.
 */
function WebGate() {
  const [authenticated, setAuthenticated] = useState(
    () => api.currentToken() !== "",
  );

  if (!authenticated) {
    return <Login onLoggedIn={() => setAuthenticated(true)} />;
  }
  return <AppShell />;
}
