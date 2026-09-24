import { useState } from "react";
import AppShell from "./layout/AppShell";
import Login from "./Login";
import * as api from "./api";

/**
 * The front door (v0.9 D2b-2).
 *
 * On the **desktop** this is a one-line component: the shell's host is in the same
 * process, so a token is never asked for. On the **Web** it is the gate — a stored
 * token opens the shell, and anything else gets the login screen.
 *
 * The gate lives here rather than inside `AppShell` on purpose: `AppShell` is where
 * `useAppStore()` is called, so rendering the shell only once there is a token is
 * what keeps the store from mounting (and firing a dozen unauthenticated requests)
 * before there is one. `api` reads any stored token when it loads, so a reload with
 * a token in `sessionStorage` goes straight through.
 */
export default function App() {
  const [authenticated, setAuthenticated] = useState(() => api.currentToken() !== "");

  if (!authenticated) {
    return <Login onLoggedIn={() => setAuthenticated(true)} />;
  }
  return <AppShell />;
}
