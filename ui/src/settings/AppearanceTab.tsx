import type { AppStore } from "../state/appStore";
import { THEMES, themeLabel, themeSummary } from "../lib/theme";
import { LANGUAGE_CHOICES, languageChoiceKey, t } from "../i18n/index.ts";

/** Appearance settings (v0.4 #11a theme, v0.7 batch 2 language). */
export default function AppearanceTab({ store }: { store: AppStore }) {
  return (
    <>
      <h3>{t("appearance.theme")}</h3>
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
          {t("appearance.cycle")}
        </button>
      </div>
      <div className="muted small">{themeSummary(store.theme, store.resolvedTheme)}</div>
      <div className="muted small">{t("appearance.hint")}</div>

      {/* Language (v0.7 batch 2): the theme's three-state shape again — follow the
          system, or pin one language. The labels come from the registry, so the
          switcher reads in the language it is switching to, and `lang` on <html>
          follows the same choice. */}
      <h3>{t("language.heading")}</h3>
      <div className="theme-choices">
        {LANGUAGE_CHOICES.map((choice) => (
          <button
            key={choice}
            className={`tab-btn${store.languageChoice === choice ? " active" : ""}`}
            onClick={() => void store.setLanguage(choice)}
          >
            {t(languageChoiceKey(choice))}
          </button>
        ))}
      </div>
    </>
  );
}
