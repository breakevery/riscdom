import type { AppStore } from "../state/appStore";

/**
 * Placeholder for the capability-plugin system (v0.4+).
 *
 * The structure exists so the tab hierarchy is settled early; nothing here is
 * implemented yet, and no plugin can be installed from this build.
 */
export default function PluginTab(_props: { store: AppStore }) {
  return (
    <>
      <div className="banner info">
        插件系统将于 v0.4 提供。核心能力宿主化，边缘能力插件化，默认拒绝。
      </div>
      <div className="muted small">
        规划中的内容：插件清单（声明所需能力）、人类逐项批准、能力默认拒绝、所有调用写入审计。
        当前版本没有任何插件可安装或启用。
      </div>
    </>
  );
}
