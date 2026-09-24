import type { AppStore } from "../state/appStore";
import { DesktopOnly, WebOnly } from "../components/DesktopOnly";
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
  switch (dl.event?.state) {
    case "started":
      return t("toolchain.download.started");
    case "progress": {
      const { downloaded, total } = dl.progress ?? { downloaded: 0, total: null };
      const mb = (n: number) => (n / 1024 / 1024).toFixed(1);
      return total
        ? t("toolchain.download.progress", {
            percent: Math.min(100, Math.round((downloaded / total) * 100)),
            done: mb(downloaded),
            total: mb(total),
          })
        : t("toolchain.download.progress_unknown", { done: mb(downloaded) });
    }
    case "verifying":
      return t("toolchain.download.verifying");
    case "extracting":
      return t("toolchain.download.extracting");
    case "done":
      return t("toolchain.download.done");
    case "cancelled":
      return t("toolchain.download.cancelled");
    case "failed":
      return t("toolchain.download.failed");
    default:
      return t("toolchain.download.idle");
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
  const kind = dl.event?.state ?? null;
  const percent =
    dl.progress && dl.progress.total
      ? Math.min(100, Math.round((dl.progress.downloaded / dl.progress.total) * 100))
      : null;

  const startDownload = () => {
    if (busy) return;
    const ok = window.confirm(t("toolchain.download.confirm"));
    if (ok) void store.startToolchainDownload();
  };

  return (
    <>
      {/* The browser is a read-only board (v0.9 D2b-4a): the status blocks below stay,
          the controls around them are the desktop's. */}
      <WebOnly>
        <div className="muted small">{t("web.readonly_note")}</div>
      </WebOnly>

      {store.toolchain && !store.toolchain.found ? (
        <div className="banner warn">{t("toolchain.missing_gcc")}</div>
      ) : null}

      <div className="status-line">
        <span className={`dot ${store.toolchain?.found ? "ok" : "bad"}`} />
        {store.toolchain?.found ? (
          <>
            RISC-V GCC · <span className="badge">{store.toolchain.source}</span>
          </>
        ) : (
          t("toolchain.not_found")
        )}
        <button className="ghost tiny" onClick={() => void store.refreshToolchain()}>
          {t("toolchain.reprobe")}
        </button>
        <DesktopOnly>
          <button
            className="ghost tiny"
            onClick={() => void store.pickToolchainPath()}
            title={t("toolchain.browse_title")}
          >
            {t("toolchain.browse")}
          </button>
          <button
            className="ghost tiny"
            onClick={() => {
              const p = window.prompt(
                t("toolchain.prompt_gcc"),
                store.toolchain?.path ?? "",
              );
              if (p) void store.setToolchain(p.trim());
            }}
          >
            {t("toolchain.manual")}
          </button>
          {store.toolchain?.path ? (
            <button className="ghost tiny" onClick={() => void store.clearToolchain()}>
              {t("toolchain.clear_manual")}
            </button>
          ) : null}
        </DesktopOnly>
      </div>

      <div className="muted small">{store.toolchain?.path ?? t("toolchain.no_path")}</div>

      <details>
        <summary className="muted small">{t("toolchain.details")}</summary>
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
          t("qemu.missing")
        )}
        <button className="ghost tiny" onClick={() => void store.refreshQemu()}>
          {t("toolchain.reprobe")}
        </button>
        <DesktopOnly>
          <button
            className="ghost tiny"
            onClick={() => void store.pickQemuPath()}
            title={t("toolchain.browse_title")}
          >
            {t("toolchain.browse")}
          </button>
          <button
            className="ghost tiny"
            onClick={() => {
              const p = window.prompt(
                t("toolchain.prompt_qemu"),
                store.qemu?.path ?? "",
              );
              if (p) void store.setQemu(p.trim());
            }}
          >
            {t("toolchain.manual")}
          </button>
          {store.qemu?.path ? (
            <button className="ghost tiny" onClick={() => void store.clearQemu()}>
              {t("toolchain.clear_manual")}
            </button>
          ) : null}
        </DesktopOnly>
      </div>
      <div className="muted small">{store.qemu?.path ?? t("toolchain.no_path")}</div>
      <details>
        <summary className="muted small">{t("toolchain.details")}</summary>
        <pre className="muted small">{store.qemu?.diagnostics ?? ""}</pre>
      </details>

      <h3>{t("toolchain.download.heading")}</h3>
      <DesktopOnly>
        <div className="row">
          <button disabled={busy} onClick={startDownload}>
            {busy ? t("toolchain.download.busy") : t("toolchain.download.button")}
          </button>
          {busy ? (
            <button className="ghost" onClick={() => void store.cancelToolchainDownload()}>
              {t("toolchain.download.cancel")}
            </button>
          ) : null}
        </div>
      </DesktopOnly>

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

      {!busy && kind === "done" && dl.event?.state === "done" ? (
        <div className="muted small">
          {t("toolchain.download.installed", { path: dl.event.install_path })}
        </div>
      ) : null}
      {!busy && kind === "cancelled" ? (
        <div className="muted small">{t("toolchain.download.cancelled_note")}</div>
      ) : null}
      {!busy && kind === "failed" ? (
        <>
          <div className="field-error">
            {dl.event?.state === "failed"
              ? dl.event.reason
              : t("toolchain.download.failed")}
          </div>
          <div className="row">
            <button className="ghost" onClick={startDownload}>
              {t("toolchain.download.retry")}
            </button>
          </div>
        </>
      ) : null}

      {/* Environment preflight (v0.4 batch 3): run the real pair once. */}
      <h3>{t("toolchain.preflight.heading")}</h3>
      <div className="status-line">
        <span
          className={`dot ${store.preflight?.ok ? "ok" : store.preflight?.checked ? "bad" : "off"}`}
        />
        <span className="small">{preflightHeadline(store.preflight)}</span>
        <DesktopOnly>
          <button className="ghost tiny" onClick={() => void store.runPreflight()}>
            {t("toolchain.preflight.rerun")}
          </button>
        </DesktopOnly>
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
      <DesktopOnly>
        {shouldOfferOverride(store.preflight) ? (
          <div className="row">
            <button className="ghost" onClick={() => void store.acknowledgePreflight()}>
              {t("toolchain.preflight.override")}
            </button>
          </div>
        ) : null}
      </DesktopOnly>
    </>
  );
}
