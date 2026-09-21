import type { AppStore } from "../state/appStore";
import {
  preflightHeadline,
  progressLine,
  rowStateLabel,
  shouldOfferOverride,
  stepLabel,
} from "../lib/preflightView";
import { navigatorPlatform, qemuInstallKey } from "../lib/platform";
import { t } from "../i18n/index.ts";

/** Human-readable state line for the download area. */
function statusText(store: AppStore): string {
  const dl = store.toolchainDownload;
  switch (dl.event?.kind) {
    case "started":
      return "开始下载…";
    case "progress": {
      const { downloaded, total } = dl.progress ?? { downloaded: 0, total: null };
      const mb = (n: number) => (n / 1024 / 1024).toFixed(1);
      return total
        ? `正在下载 ${Math.min(100, Math.round((downloaded / total) * 100))}%（${mb(downloaded)} MB / ${mb(total)} MB）`
        : `正在下载 ${mb(downloaded)} MB…`;
    }
    case "verifying":
      return "校验 SHA-256…";
    case "extracting":
      return "解压中…";
    case "done":
      return "完成";
    case "cancelled":
      return "已取消";
    case "failed":
      return "下载失败";
    default:
      return "空闲";
  }
}

/**
 * RISC-V toolchain status plus the one-click download (v0.3 #3).
 *
 * The toolchain itself is resolved by the host (stages 24a–24c); the download is
 * explicit — nothing is fetched until the button is pressed.
 */
export default function ToolchainTab({ store }: { store: AppStore }) {
  const dl = store.toolchainDownload;
  const busy = dl.in_progress;
  const kind = dl.event?.kind ?? null;
  const percent =
    dl.progress && dl.progress.total
      ? Math.min(100, Math.round((dl.progress.downloaded / dl.progress.total) * 100))
      : null;

  const startDownload = () => {
    if (busy) return;
    const ok = window.confirm(
      "将下载约 200 MB 的 xPack RISC-V GCC 并解压到应用数据目录，是否继续？",
    );
    if (ok) void store.startToolchainDownload();
  };

  return (
    <>
      {store.toolchain && !store.toolchain.found ? (
        <div className="banner warn">
          未找到 RISC-V GCC。可点下方“一键下载”自动安装，或手动安装后指定路径（见
          docs/toolchain-setup.md）。
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
          onClick={() => void store.pickToolchainPath()}
          title="用系统文件选择器指定"
        >
          浏览…
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
          手动输入
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

      <h3>QEMU</h3>
      {store.qemu && !store.qemu.found ? (
        <div className="banner warn">
          {t("qemu.missing")} {t(qemuInstallKey(navigatorPlatform()))}
        </div>
      ) : null}
      <div className="status-line">
        <span className={`dot ${store.qemu?.found ? "ok" : "bad"}`} />
        {store.qemu?.found ? (
          <>
            QEMU · <span className="badge">{store.qemu.source}</span>
          </>
        ) : (
          "未找到 QEMU"
        )}
        <button className="ghost tiny" onClick={() => void store.refreshQemu()}>
          重新探测
        </button>
        <button
          className="ghost tiny"
          onClick={() => void store.pickQemuPath()}
          title="用系统文件选择器指定"
        >
          浏览…
        </button>
        <button
          className="ghost tiny"
          onClick={() => {
            const p = window.prompt(
              "qemu-system-riscv64.exe 的完整路径",
              store.qemu?.path ?? "",
            );
            if (p) void store.setQemu(p.trim());
          }}
        >
          手动输入
        </button>
        {store.qemu?.path ? (
          <button className="ghost tiny" onClick={() => void store.clearQemu()}>
            清除手动路径
          </button>
        ) : null}
      </div>
      <div className="muted small">{store.qemu?.path ?? "（未解析到路径）"}</div>
      <details>
        <summary className="muted small">探测详情</summary>
        <pre className="muted small">{store.qemu?.diagnostics ?? ""}</pre>
      </details>

      <h3>一键下载</h3>
      <div className="row">
        <button disabled={busy} onClick={startDownload}>
          {busy ? "下载中…" : "一键下载 RISC-V GCC"}
        </button>
        {busy ? (
          <button className="ghost" onClick={() => void store.cancelToolchainDownload()}>
            取消
          </button>
        ) : null}
      </div>

      {busy ? (
        <>
          <div className="progress">
            <div
              className="progress-fill"
              style={{ width: percent === null ? "30%" : `${percent}%` }}
            />
          </div>
          <div className="muted small">{statusText(store)}</div>
        </>
      ) : null}

      {!busy && kind === "done" && dl.event?.kind === "done" ? (
        <div className="muted small">已安装到 {dl.event.install_path}</div>
      ) : null}
      {!busy && kind === "cancelled" ? <div className="muted small">已取消下载</div> : null}
      {!busy && kind === "failed" ? (
        <>
          <div className="field-error">
            {dl.event?.kind === "failed" ? dl.event.reason : "下载失败"}
          </div>
          <div className="row">
            <button className="ghost" onClick={startDownload}>
              重试
            </button>
          </div>
        </>
      ) : null}

      {/* Environment preflight (v0.4 batch 3): run the real pair once. */}
      <h3>环境预检</h3>
      <div className="status-line">
        <span
          className={`dot ${store.preflight?.ok ? "ok" : store.preflight?.checked ? "bad" : "off"}`}
        />
        <span className="small">{preflightHeadline(store.preflight)}</span>
        <button className="ghost tiny" onClick={() => void store.runPreflight()}>
          重新预检
        </button>
      </div>
      {store.preflightStep ? (
        <div className="muted small">
          {progressLine(store.preflightStep.step, store.preflightStep.state)}
        </div>
      ) : null}
      <ul className="audit-list">
        {(store.preflight?.rows ?? []).map((r) => (
          <li key={r.step}>
            <span className={`badge ${r.state === "ok" ? "ok" : ""}`}>
              {rowStateLabel(r.state)}
            </span>
            <span className="action">{stepLabel(r.step)}</span>
          </li>
        ))}
      </ul>
      {store.preflight?.detail ? (
        <pre className="muted small">{store.preflight.detail}</pre>
      ) : null}
      {store.preflight?.suggestion ? (
        <div className="banner warn">{store.preflight.suggestion}</div>
      ) : null}
      {shouldOfferOverride(store.preflight) ? (
        <div className="row">
          <button className="ghost" onClick={() => void store.acknowledgePreflight()}>
            仍要继续（记录该选择）
          </button>
        </div>
      ) : null}
    </>
  );
}
