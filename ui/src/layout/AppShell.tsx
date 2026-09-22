import { useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { useAppStore } from "../state/appStore";
import { t } from "../i18n/index.ts";
import {
  RESIZER_W,
  clampChatWidth,
  maxChatWidth,
  serialMinWidth,
} from "../lib/chatWidth";
import CanvasPanel from "../panels/CanvasPanel";
import ChatPanel from "../panels/ChatPanel";
import SettingsPanel from "../panels/SettingsPanel";

/** Only a non-sensitive layout preference is stored here. */
const CHAT_WIDTH_KEY = "riscdom.layout.chatWidth";
const DEFAULT_CHAT_W = 380;
const MIN_CHAT_W = 240;
const MAX_CHAT_W = 900;

type View = "main" | "settings";

function readChatWidth(): number {
  try {
    const raw = window.localStorage.getItem(CHAT_WIDTH_KEY);
    const value = raw === null ? Number.NaN : Number.parseInt(raw, 10);
    if (Number.isFinite(value) && value >= MIN_CHAT_W && value <= MAX_CHAT_W) {
      return value;
    }
  } catch {
    /* ignore */
  }
  return DEFAULT_CHAT_W;
}

function writeChatWidth(width: number): void {
  try {
    window.localStorage.setItem(CHAT_WIDTH_KEY, String(Math.round(width)));
  } catch {
    /* ignore */
  }
}

/** Human-readable elapsed time for the VM badge tooltip. */
function elapsedLabel(sinceMs: number | null): string {
  if (sinceMs === null) return "";
  const seconds = Math.max(0, Math.round((Date.now() - sinceMs) / 1000));
  if (seconds < 60) return t("app.vm.uptime.seconds", { n: seconds });
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return t("app.vm.uptime.minutes", { n: minutes });
  return t("app.vm.uptime.hours", { n: (minutes / 60).toFixed(1) });
}

export default function AppShell() {
  const store = useAppStore();
  const [view, setView] = useState<View>("main");
  const [chatW, setChatW] = useState<number>(readChatWidth);
  const drag = useRef<{ startX: number; startW: number; width: number } | null>(null);

  // The two columns must fit whatever the window gives us (v0.3.1 #5): the drag
  // bound is derived from the measured container instead of an absolute
  // maximum, which used to push the serial column off-screen in a narrow window.
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const [bodyW, setBodyW] = useState(0);

  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const update = () => setBodyW(el.clientWidth);
    update();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", update);
      return () => window.removeEventListener("resize", update);
    }
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const chatMaxW = maxChatWidth(bodyW, MIN_CHAT_W, MAX_CHAT_W);
  const chatColW = clampChatWidth(chatW, bodyW, MIN_CHAT_W, MAX_CHAT_W);
  const serialMin = serialMinWidth(bodyW, chatColW);

  // Esc leaves the settings page.
  useEffect(() => {
    if (view !== "settings") return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setView("main");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view]);

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    drag.current = { startX: e.clientX, startW: chatColW, width: chatColW };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const delta = e.clientX - d.startX;
    const next = Math.max(MIN_CHAT_W, Math.min(chatMaxW, d.startW + delta));
    d.width = next;
    setChatW(next);
  };

  const onPointerUp = () => {
    const d = drag.current;
    drag.current = null;
    if (d) writeChatWidth(d.width);
  };

  return (
    <div className="app-root">
      <header className="topbar">
        {view === "settings" ? (
          <>
            <button className="ghost tiny" onClick={() => setView("main")}>
              {t("app.back")}
            </button>
            <span className="spacer" />
            <span className="brand">RiscDom</span>
          </>
        ) : (
          <>
            <span className="brand">RiscDom</span>
            <span className="spacer" />
            {store.vmSeen ? (
              <span
                className="vm-badge"
                title={
                  store.vmStatus.running
                    ? t("app.vm.badge_title", {
                        uptime: elapsedLabel(store.vmStatus.sinceMs),
                      })
                    : t("app.vm.stopped")
                }
              >
                <span className={`dot ${store.vmStatus.running ? "ok" : "off"}`} />
                {store.vmStatus.running ? t("app.vm.running") : t("app.vm.stopped")}
              </span>
            ) : null}
            <button
              className="ghost tiny gear"
              title={t("app.settings_title")}
              aria-label={t("settings.heading")}
              onClick={() => setView("settings")}
            >
              {/* Inline gear icon (no icon library). */}
              <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
                <path
                  fill="currentColor"
                  d="M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm0 6a2 2 0 1 1 0-4 2 2 0 0 1 0 4Z"
                />
                <path
                  fill="currentColor"
                  d="M19.4 13a7.7 7.7 0 0 0 .1-1 7.7 7.7 0 0 0-.1-1l2-1.6a.5.5 0 0 0 .1-.6l-1.9-3.3a.5.5 0 0 0-.6-.2l-2.4 1a7.6 7.6 0 0 0-1.7-1l-.4-2.5a.5.5 0 0 0-.5-.4h-3.8a.5.5 0 0 0-.5.4l-.4 2.5a7.6 7.6 0 0 0-1.7 1l-2.4-1a.5.5 0 0 0-.6.2L2.1 8.8a.5.5 0 0 0 .1.6L4.2 11a7.7 7.7 0 0 0 0 2l-2 1.6a.5.5 0 0 0-.1.6l1.9 3.3a.5.5 0 0 0 .6.2l2.4-1a7.6 7.6 0 0 0 1.7 1l.4 2.5a.5.5 0 0 0 .5.4h3.8a.5.5 0 0 0 .5-.4l.4-2.5a7.6 7.6 0 0 0 1.7-1l2.4 1a.5.5 0 0 0 .6-.2l1.9-3.3a.5.5 0 0 0-.1-.6L19.4 13Z"
                />
              </svg>
            </button>
          </>
        )}
      </header>

      <div className="app-body" ref={bodyRef}>
        {/* Both views stay mounted; only visibility changes, so chat/serial
            state (messages, terminal buffer, scroll position) is preserved. */}
        <div
          className="shell"
          style={{
            display: view === "main" ? "grid" : "none",
            gridTemplateColumns: `${chatColW}px ${RESIZER_W}px minmax(${serialMin}px, 1fr)`,
            overflow: "hidden",
          }}
        >
          <ChatPanel store={store} />
          <div
            className="resizer"
            role="separator"
            aria-orientation="vertical"
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
          />
          <CanvasPanel store={store} />
        </div>

        <div
          className="settings-page"
          style={{ display: view === "settings" ? "flex" : "none" }}
        >
          <SettingsPanel store={store} />
        </div>
      </div>
    </div>
  );
}
