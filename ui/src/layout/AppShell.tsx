import { useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { useAppStore } from "../state/appStore";
import CanvasPanel from "../panels/CanvasPanel";
import ChatPanel from "../panels/ChatPanel";
import SettingsPanel from "../panels/SettingsPanel";

type Which = "chat" | "canvas";

export default function AppShell() {
  const store = useAppStore();
  const [chatW, setChatW] = useState(380);
  const [canvasW, setCanvasW] = useState(520);
  const drag = useRef<{ which: Which; startX: number; startW: number } | null>(
    null,
  );

  const onPointerDown =
    (which: Which) => (e: ReactPointerEvent<HTMLDivElement>) => {
      drag.current = {
        which,
        startX: e.clientX,
        startW: which === "chat" ? chatW : canvasW,
      };
      e.currentTarget.setPointerCapture(e.pointerId);
    };

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const delta = e.clientX - d.startX;
    const clamp = (v: number) => Math.max(240, Math.min(900, v));
    if (d.which === "chat") setChatW(clamp(d.startW + delta));
    else setCanvasW(clamp(d.startW - delta));
  };

  const onPointerUp = () => {
    drag.current = null;
  };

  return (
    <div
      className="shell"
      style={{
        gridTemplateColumns: `${chatW}px 6px minmax(260px, 1fr) 6px ${canvasW}px`,
      }}
    >
      <ChatPanel store={store} />
      <div
        className="resizer"
        role="separator"
        aria-orientation="vertical"
        onPointerDown={onPointerDown("chat")}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      />
      <SettingsPanel store={store} />
      <div
        className="resizer"
        role="separator"
        aria-orientation="vertical"
        onPointerDown={onPointerDown("canvas")}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      />
      <CanvasPanel store={store} />
    </div>
  );
}
