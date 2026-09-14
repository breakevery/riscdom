import type { AppStore } from "../state/appStore";

// Stage 6b: placeholder. Stage 6c replaces this with the xterm.js canvas.
export default function CanvasPanel({ store }: { store: AppStore }) {
  return (
    <section className="panel">
      <header className="panel-head">串口画布</header>
      <div className="canvas-placeholder">
        xterm.js 串口画布将在阶段 6c 接入。
        <br />
        当前 VM 状态：<b>{store.vmState}</b>
      </div>
    </section>
  );
}
