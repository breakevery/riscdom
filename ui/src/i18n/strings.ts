/**
 * UI string registry (v0.7 batch 1; the settings group followed in v0.8).
 *
 * The self-built i18n facility's data: one table per language, the same keys in
 * each, so a missing or empty translation is visible in this file itself. English
 * declares the key set (and is the fallback); its wording for the v0.6 diff block
 * mirrors the table `ui/README.md` kept "so it can be mirrored when the interface
 * becomes translatable".
 *
 * Keys are flat and dot-separated: `<domain>.<key>`, or `<domain>.<sub>.<key>`
 * where a domain has a real second level (`qemu.install.windows`,
 * `toolchain.download.started`). A key carries `{name}` placeholders when the
 * sentence needs one; `t()` fills them and leaves an unknown one as written.
 *
 * Dependency-free on purpose, like `lib/theme.ts`: `ui/scripts/probe-ui-runs.mjs`
 * and `scripts/check-ui-strings.mjs` import this module directly (Node strips the
 * types), so it may not reach for React or the DOM.
 */

/** A language the interface can be shown in. */
export type Language = "en" | "zh";

/** Every language the registry carries, the fallback first. */
export const LANGUAGES: readonly Language[] = ["en", "zh"];

/** English defines the key set; every other language must match it exactly. */
const EN = {
  "diff.headline": "field-level differences · {fields} fields · {different} differ",
  "diff.loading": "reading the differences…",
  "diff.failed": "could not read the fingerprint diff: {reason}",
  "diff.empty": "these two runs' fingerprints share no field to compare.",
  "language.heading": "Language",
  "language.system": "Follow the system",
  "language.english": "English",
  "language.chinese": "中文",
  "qemu.missing": "QEMU (qemu-system-riscv64) was not found.",
  "qemu.install.windows":
    "On Windows, run `winget install SoftwareFreedomConservancy.QEMU` or install from https://www.qemu.org/download/#windows; you can also set the full path by hand.",
  "qemu.install.macos":
    "On macOS, run `brew install qemu` or install from https://www.qemu.org/download/; you can also set the full path by hand.",
  "qemu.install.linux":
    "On Linux, install your distribution's package (`qemu-system-misc` on Debian/Ubuntu, `qemu` on Arch/Fedora), or build from https://www.qemu.org/download/; you can also set the full path by hand.",
  "qemu.install.other":
    "Install QEMU from https://www.qemu.org/download/; you can also set the full path by hand.",

  // Settings chrome (v0.8, settings group).
  "settings.heading": "Settings",
  "settings.tab.model": "Model",
  "settings.tab.toolchain": "Toolchain",
  "settings.tab.snapshot": "Snapshots",
  "settings.tab.audit": "Audit",
  "settings.tab.plugin": "Plugins",
  "settings.tab.appearance": "Appearance",
  "appearance.theme": "Theme",
  "appearance.cycle": "Cycle",
  "appearance.hint":
    "The choice is written to settings.json and takes effect again after a restart.",
  "plugin.intro":
    "The plugin system ships in v0.4. Core capabilities live in the host, edge capabilities are plugins, and the default is to deny.",
  "plugin.planned":
    "Planned: a plugin manifest (declaring the capabilities it needs), human approval item by item, capabilities denied by default, and every call written to the audit log. No plugin can be installed or enabled in this build.",

  // Toolchain tab (v0.8, settings group).
  "toolchain.download.started": "Starting the download…",
  "toolchain.download.progress": "Downloading {percent}% ({done} MB / {total} MB)",
  "toolchain.download.progress_unknown": "Downloading {done} MB…",
  "toolchain.download.verifying": "Verifying SHA-256…",
  "toolchain.download.extracting": "Extracting…",
  "toolchain.download.done": "Done",
  "toolchain.download.cancelled": "Cancelled",
  "toolchain.download.failed": "Download failed",
  "toolchain.download.idle": "Idle",
  "toolchain.download.confirm":
    "This downloads about 200 MB of xPack RISC-V GCC and extracts it into the app data directory. Continue?",
  "toolchain.missing_gcc":
    "RISC-V GCC was not found. Use the one-click download below to install it, or install one yourself and point RiscDom at it (see docs/toolchain-setup.md).",
  "toolchain.not_found": "Toolchain not found",
  "toolchain.reprobe": "Detect again",
  "toolchain.browse": "Browse…",
  "toolchain.browse_title": "Pick it with the system file dialog",
  "toolchain.prompt_gcc": "Full path to riscv64-unknown-elf-gcc.exe",
  "toolchain.prompt_qemu": "Full path to qemu-system-riscv64.exe",
  "toolchain.manual": "Enter by hand",
  "toolchain.clear_manual": "Clear the manual path",
  "toolchain.no_path": "(no path resolved)",
  "toolchain.details": "Detection details",
  "toolchain.download.heading": "One-click download",
  "toolchain.download.busy": "Downloading…",
  "toolchain.download.button": "Download RISC-V GCC",
  "toolchain.download.cancel": "Cancel",
  "toolchain.download.installed": "Installed to {path}",
  "toolchain.download.cancelled_note": "Download cancelled",
  "toolchain.download.retry": "Retry",
  "toolchain.preflight.heading": "Environment preflight",
  "toolchain.preflight.rerun": "Run it again",
  "toolchain.preflight.override": "Continue anyway (records the choice)",

  // Model tab (v0.8, settings group).
  "model.banner.no_config":
    "No model is configured yet. Pick a provider and enter an API key, or use a local model.",
  "model.banner.missing_api_key":
    "The API key is missing. Enter one, or switch to a local-model preset.",
  "model.banner.invalid_base_url": "The base URL is not valid. Check it.",
  "model.banner.invalid_config":
    "The configuration is not valid. Check the provider and the model.",
  "model.not_ready": "The model is not ready. Check the configuration.",
  "model.presets_failed": "Could not load the providers",
  "model.key_restored": "Key restored from the system keyring",
  "model.key_loaded": "Key loaded from the system keyring",
  "model.saved_keyring": "Saved for this session and written to the system keyring",
  "model.saved_memory": "Saved for this session (memory only)",
  "model.cleared": "Cleared (including the system keyring entry)",
  "model.no_local": "No local model was detected",
  "model.switched_local": "Switched to the local model",
  "model.detecting": "Checking…",
  "model.detect_local": "Check for a local model",
  "model.stored_prompt": "A saved key for {provider} was found. Load it?",
  "model.load": "Load",
  "model.ignore": "Ignore",
  "model.local_found": "A local model was found: {name} ({url}, {count} models). Use it?",
  "model.use": "Use",
  "model.provider": "Provider",
  "model.no_key_needed": "A local model needs no key",
  "model.key_placeholder": "sk-… (kept in memory only)",
  "model.remember": "Save to the system keyring (recommended)",
  "model.remember_hint": "With this unchecked, the API key is kept for this session only.",
  "model.save": "Save for this session",
  "model.clear": "Clear",
  "model.status_configured": "Configured · {provider} · {state}",
  "model.status_persisted": "saved to the system keyring",
  "model.status_session_only": "this session only",
  "model.status_unconfigured": "Not configured (the API key is never written to disk)",

  // Audit tab (v0.8, settings group).
  "audit.status": "{count} events · {state}",
  "audit.chain_intact": "chain intact ({length})",
  "audit.chain_broken": "chain broken @{id}",
  "audit.unknown": "unknown",
  "audit.loading": "Loading…",
  "audit.refresh": "Refresh",
  "audit.filter_actor": "Filter by actor",
  "audit.runs_heading": "Recent runs",
  "audit.compare_hint": "Tick two to compare their fingerprints side by side",
  "audit.clear_selection": "Clear the selection",
  "audit.select_run": "Select {id}",
  "audit.export": "Export",

  // Snapshot tab (v0.8, settings group).
  "snapshot.count": "{count} snapshots",
  "snapshot.refresh": "Refresh",
  "snapshot.save_title": "Save the current VM state",
  "snapshot.save_title_disabled": "Needs a running VM (let the AI start one first)",
  "snapshot.name_prompt": "Snapshot name (letters/digits/-/_)",
  "snapshot.save": "Save the current state",
  "snapshot.vm_running": "running (host-owned, reused across runs)",
  "snapshot.vm_stopped": "not running",
  "snapshot.vm_note":
    "Save/restore is a real snapshot (tcp-relay) and needs the host to hold the VM.",
  "snapshot.mode_real": "real",
  "snapshot.mode_reboot": "reboot-style",
  "snapshot.restore_confirm": 'Restore from snapshot "{name}"? The current VM will be stopped.',
  "snapshot.restore": "Restore",
  "snapshot.delete_confirm": 'Delete snapshot "{name}"?',
  "snapshot.delete": "Delete",
  "snapshot.workspace": "Workspace",
  "snapshot.files": "{count} files",
} as const;

/** A key of the registry. */
export type StringKey = keyof typeof EN;

/** The other language: every key of `EN`, and nothing else. */
const ZH: Record<StringKey, string> = {
  "diff.headline": "字段级差异 · {fields} 个字段 · {different} 处不同",
  "diff.loading": "差异读取中…",
  "diff.failed": "指纹差异读取失败：{reason}",
  "diff.empty": "这两个 run 的指纹里没有可比字段。",
  "language.heading": "语言",
  "language.system": "跟随系统",
  "language.english": "English",
  "language.chinese": "中文",
  "qemu.missing": "未找到 QEMU（qemu-system-riscv64）。",
  "qemu.install.windows":
    "Windows 可运行 `winget install SoftwareFreedomConservancy.QEMU`，或从 https://www.qemu.org/download/#windows 安装；也可手动指定完整路径。",
  "qemu.install.macos":
    "macOS 可运行 `brew install qemu`，或从 https://www.qemu.org/download/ 安装；也可手动指定完整路径。",
  "qemu.install.linux":
    "Linux 可用发行版包（Debian/Ubuntu 为 `qemu-system-misc`，Arch/Fedora 为 `qemu`），或从 https://www.qemu.org/download/ 自行构建；也可手动指定完整路径。",
  "qemu.install.other":
    "从 https://www.qemu.org/download/ 安装 QEMU；也可手动指定完整路径。",

  // Settings chrome (v0.8, settings group).
  "settings.heading": "设置",
  "settings.tab.model": "模型",
  "settings.tab.toolchain": "工具链",
  "settings.tab.snapshot": "快照",
  "settings.tab.audit": "审计",
  "settings.tab.plugin": "插件",
  "settings.tab.appearance": "外观",
  "appearance.theme": "主题",
  "appearance.cycle": "循环切换",
  "appearance.hint": "选择会写入 settings.json，重启后继续生效。",
  "plugin.intro": "插件系统将于 v0.4 提供。核心能力宿主化，边缘能力插件化，默认拒绝。",
  "plugin.planned":
    "规划中的内容：插件清单（声明所需能力）、人类逐项批准、能力默认拒绝、所有调用写入审计。当前版本没有任何插件可安装或启用。",

  // Toolchain tab (v0.8, settings group).
  "toolchain.download.started": "开始下载…",
  "toolchain.download.progress": "正在下载 {percent}%（{done} MB / {total} MB）",
  "toolchain.download.progress_unknown": "正在下载 {done} MB…",
  "toolchain.download.verifying": "校验 SHA-256…",
  "toolchain.download.extracting": "解压中…",
  "toolchain.download.done": "完成",
  "toolchain.download.cancelled": "已取消",
  "toolchain.download.failed": "下载失败",
  "toolchain.download.idle": "空闲",
  "toolchain.download.confirm":
    "将下载约 200 MB 的 xPack RISC-V GCC 并解压到应用数据目录，是否继续？",
  "toolchain.missing_gcc":
    "未找到 RISC-V GCC。可点下方“一键下载”自动安装，或手动安装后指定路径（见 docs/toolchain-setup.md）。",
  "toolchain.not_found": "未找到工具链",
  "toolchain.reprobe": "重新探测",
  "toolchain.browse": "浏览…",
  "toolchain.browse_title": "用系统文件选择器指定",
  "toolchain.prompt_gcc": "riscv64-unknown-elf-gcc.exe 的完整路径",
  "toolchain.prompt_qemu": "qemu-system-riscv64.exe 的完整路径",
  "toolchain.manual": "手动输入",
  "toolchain.clear_manual": "清除手动路径",
  "toolchain.no_path": "（未解析到路径）",
  "toolchain.details": "探测详情",
  "toolchain.download.heading": "一键下载",
  "toolchain.download.busy": "下载中…",
  "toolchain.download.button": "一键下载 RISC-V GCC",
  "toolchain.download.cancel": "取消",
  "toolchain.download.installed": "已安装到 {path}",
  "toolchain.download.cancelled_note": "已取消下载",
  "toolchain.download.retry": "重试",
  "toolchain.preflight.heading": "环境预检",
  "toolchain.preflight.rerun": "重新预检",
  "toolchain.preflight.override": "仍要继续（记录该选择）",

  // Model tab (v0.8, settings group).
  "model.banner.no_config": "尚未配置模型。选择服务商并填写 API Key，或使用本地模型。",
  "model.banner.missing_api_key": "缺少 API Key。请填写，或切换到本地模型预设。",
  "model.banner.invalid_base_url": "Base URL 无效，请检查。",
  "model.banner.invalid_config": "配置无效，请检查服务商与 Model。",
  "model.not_ready": "模型未就绪，请检查配置。",
  "model.presets_failed": "加载服务商失败",
  "model.key_restored": "已从系统钥匙串恢复 Key",
  "model.key_loaded": "已从系统钥匙串加载 Key",
  "model.saved_keyring": "已保存到本次会话并写入系统钥匙串",
  "model.saved_memory": "已保存到本次会话（仅内存）",
  "model.cleared": "已清除（含系统钥匙串条目）",
  "model.no_local": "未检测到本地模型",
  "model.switched_local": "已切换到本地模型",
  "model.detecting": "检测中…",
  "model.detect_local": "检测本地模型",
  "model.stored_prompt": "检测到已保存的 {provider} Key，是否加载？",
  "model.load": "加载",
  "model.ignore": "忽略",
  "model.local_found": "检测到本地模型 {name}（{url}，{count} 个模型）。是否使用？",
  "model.use": "使用",
  "model.provider": "服务商",
  "model.no_key_needed": "本地模型无需 key",
  "model.key_placeholder": "sk-…（仅保存在内存）",
  "model.remember": "保存到系统钥匙串（推荐）",
  "model.remember_hint": "未勾选时，API Key 仅保存在本次会话。",
  "model.save": "保存到本次会话",
  "model.clear": "清除",
  "model.status_configured": "已配置 · {provider} · {state}",
  "model.status_persisted": "已保存到系统钥匙串",
  "model.status_session_only": "仅本次会话",
  "model.status_unconfigured": "未配置（API key 不会落盘）",

  // Audit tab (v0.8, settings group).
  "audit.status": "{count} 条事件 · {state}",
  "audit.chain_intact": "链完整 ({length})",
  "audit.chain_broken": "链断裂 @{id}",
  "audit.unknown": "未知",
  "audit.loading": "加载中…",
  "audit.refresh": "刷新",
  "audit.filter_actor": "按 actor 过滤",
  "audit.runs_heading": "最近运行（run）",
  "audit.compare_hint": "勾选两个可并排对比指纹",
  "audit.clear_selection": "清除选择",
  "audit.select_run": "选择 {id}",
  "audit.export": "导出",

  // Snapshot tab (v0.8, settings group).
  "snapshot.count": "{count} 个快照",
  "snapshot.refresh": "刷新",
  "snapshot.save_title": "保存当前 VM 状态",
  "snapshot.save_title_disabled": "需要运行中的 VM（先让 AI 启动一台）",
  "snapshot.name_prompt": "快照名称（字母/数字/-/_）",
  "snapshot.save": "保存当前状态",
  "snapshot.vm_running": "运行中（host 持有，跨 run 复用）",
  "snapshot.vm_stopped": "未运行",
  "snapshot.vm_note": "保存/恢复为真实快照（tcp-relay），需由 host 持有 VM。",
  "snapshot.mode_real": "真实",
  "snapshot.mode_reboot": "重启式",
  "snapshot.restore_confirm": "从快照“{name}”恢复？当前 VM 会被停止。",
  "snapshot.restore": "恢复",
  "snapshot.delete_confirm": "删除快照“{name}”？",
  "snapshot.delete": "删除",
  "snapshot.workspace": "工作区",
  "snapshot.files": "{count} 个文件",
};

/** The registry itself: language -> key -> template. */
export const STRINGS: Record<Language, Record<StringKey, string>> = { en: EN, zh: ZH };
