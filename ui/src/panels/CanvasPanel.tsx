import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import * as api from "../api/tauri";
import type { AppStore } from "../state/appStore";

const TERMINAL_THEME = {
  background: "#0b0f14",
  foreground: "#d7e0ea",
  cursor: "#58a6ff",
};

export default function CanvasPanel({ store }: { store: AppStore }) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const [note, setNote] = useState<string | null>(null);

  // ----- auto-scroll (v0.3 #1) --------------------------------------------
  const stickRef = useRef(true); // following the latest serial output?
  const [showJump, setShowJump] = useState(false);

  const jumpToLatest = () => {
    stickRef.current = true;
    setShowJump(false);
    termRef.current?.scrollToBottom();
  };

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const term = new Terminal({
      convertEol: true,
      fontSize: 13,
      cursorBlink: true,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
      theme: TERMINAL_THEME,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    try {
      fit.fit();
    } catch {
      /* layout not ready yet */
    }
    termRef.current = term;
    fitRef.current = fit;

    // Seed with whatever was captured before this panel mounted.
    api
      .getSerialBuffer()
      .then((text) => {
        if (!text) return;
        term.write(text);
        term.scrollToBottom();
      })
      .catch(() => {});

    // Live increments: follow unless the user has scrolled up.
    const off = api.onHostEvent("serial:chunk", (p) => {
      const d = p as { chunk?: string };
      if (!d.chunk) return;
      term.write(d.chunk);
      if (stickRef.current) {
        term.scrollToBottom();
      } else {
        setShowJump(true);
      }
    });

    // Detect manual scrolling: `viewportY === baseY` means "at the bottom".
    const scrollSub = term.onScroll(() => {
      const buf = term.buffer.active;
      const atBottom = buf.viewportY >= buf.baseY;
      stickRef.current = atBottom;
      if (atBottom) setShowJump(false);
    });

    const ro = new ResizeObserver(() => {
      try {
        fit.fit();
      } catch {
        /* ignore */
      }
    });
    ro.observe(host);

    return () => {
      off();
      scrollSub.dispose();
      ro.disconnect();
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, []);

  const clear = () => {
    const term = termRef.current;
    if (!term) return;
    term.clear();
    term.write("\x1b[2J\x1b[H");
    stickRef.current = true;
    setShowJump(false);
  };

  const exportLog = async () => {
    const name = `serial-${new Date()
      .toISOString()
      .replace(/[:.]/g, "-")}.log`;
    try {
      const bytes = await api.exportSerialLog(name);
      setNote(`已导出 ${name}（${bytes} 字节）`);
    } catch (e) {
      setNote(`导出失败：${String(e)}`);
    }
  };

  return (
    <section className="panel">
      <header className="panel-head canvas-head">
        <span>串口画布</span>
        <span className={`vm-state ${store.vmState}`}>{store.vmState}</span>
        <span className="spacer" />
        <button className="ghost tiny" onClick={clear}>
          清屏
        </button>
        <button className="ghost tiny" onClick={() => void exportLog()}>
          导出串口日志
        </button>
      </header>
      {note ? <div className="muted small canvas-note">{note}</div> : null}
      <div className="canvas-wrap">
        <div className="canvas-host" ref={hostRef} />
        {showJump ? (
          <button className="ghost tiny jump-latest" onClick={jumpToLatest}>
            跳到最新 ↓
          </button>
        ) : null}
      </div>
    </section>
  );
}
