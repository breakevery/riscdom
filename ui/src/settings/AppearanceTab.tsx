import type { AppStore } from "../state/appStore";
import { THEMES, themeLabel, themeSummary } from "../lib/theme";

/** Appearance settings (v0.4 #11a): light / dark / follow the system. */
export default function AppearanceTab({ store }: { store: AppStore }) {
  return (
    <>
      <h3>主题</h3>
      <div className="theme-choices">
        {THEMES.map((candidate) => (
          <button
            key={candidate}
            className={`tab-btn${store.theme === candidate ? " active" : ""}`}
            onClick={() => void store.setTheme(candidate)}
          >
            {themeLabel(candidate)}
          </button>
        ))}
        <button className="ghost tiny" onClick={() => void store.cycleTheme()}>
          循环切换
        </button>
      </div>
      <div className="muted small">{themeSummary(store.theme, store.resolvedTheme)}</div>
      <div className="muted small">选择会写入 settings.json，重启后继续生效。</div>
    </>
  );
}
