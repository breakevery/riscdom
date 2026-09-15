import type { AppStore } from "../state/appStore";

/**
 * RISC-V toolchain status. The toolchain is resolved by the host (stages
 * 24a–24c); this tab shows what was found and lets the user override the path.
 */
export default function ToolchainTab({ store }: { store: AppStore }) {
  return (
    <>
      {store.toolchain && !store.toolchain.found ? (
        <div className="banner warn">
          未找到 RISC-V GCC。请安装 xPack RISC-V GCC（
          https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases
          ），或点“手动指定”填入 riscv64-unknown-elf-gcc.exe 的完整路径。
        </div>
      ) : null}

      <div className="status-line">
        <span className={`dot ${store.toolchain?.found ? "ok" : "bad"}`} />
        {store.toolchain?.found ? (
          <>
            RISC-V GCC · <span className="badge">{store.toolchain.source}</span>
          </>
        ) : (
          "未找到工具链"
        )}
        <button className="ghost tiny" onClick={() => void store.refreshToolchain()}>
          重新探测
        </button>
        <button
          className="ghost tiny"
          onClick={() => {
            const p = window.prompt(
              "riscv64-unknown-elf-gcc.exe 的完整路径",
              store.toolchain?.path ?? "",
            );
            if (p) void store.setToolchain(p.trim());
          }}
        >
          手动指定
        </button>
        {store.toolchain?.path ? (
          <button className="ghost tiny" onClick={() => void store.clearToolchain()}>
            清除手动路径
          </button>
        ) : null}
      </div>

      <div className="muted small">{store.toolchain?.path ?? "（未解析到路径）"}</div>

      <details>
        <summary className="muted small">探测详情</summary>
        <pre className="muted small">{store.toolchain?.diagnostics ?? ""}</pre>
      </details>

      {/* Reserved for a future one-click toolchain download (v0.3+). */}
      <div className="muted small">
        一键下载安装工具链：规划中（当前请按 docs/toolchain-setup.md 手动安装后重新探测）。
      </div>
    </>
  );
}
