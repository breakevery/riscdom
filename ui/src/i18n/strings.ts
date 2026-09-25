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
  "settings.tab.network": "Network",
  // The network face (v0.9.9 内网接入): two directions on one page — "out" points
  // this desktop at an in-network server, "in" serves this node to the network.
  "network.out_heading": "Connect out",
  "network.out_hint":
    "Point this desktop at an in-network RiscDom server, so one board can oversee several nodes. The connection itself arrives in the next batch.",
  "network.remote_url": "Server address",
  "network.remote_token": "Token",
  "network.connect": "Connect",
  "network.in_heading": "Serve in",
  "network.lan_enabled": "Serve this node's board on the network",
  "network.lan_bind": "Bind address",
  "network.lan_bind_hint":
    "Loopback only, until the switch below opens it to the network.",
  "network.allow_lan": "Allow other devices on this network",
  "network.allow_lan_warning":
    "Anyone on this network will be able to reach this node, and only the token stands in the way — keep it to yourself.",
  "network.lan_url": "Board address",
  "network.lan_url_pending": "not serving yet",
  "network.lan_state_running": "serving",
  "network.firewall_hint":
    "Windows may ask whether to let this app use the network the first time — choose Allow, or a phone cannot get through.",
  "network.token_show": "Show token",
  "network.token_copy": "Copy",
  "network.token_path":
    "The token lives in <data-dir>/token, inside this node's data directory.",
  "network.save": "Save",
  "network.saved": "Saved",
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
  "audit.alert.heading": "Some audit writes failed and were not recorded:",
  "audit.alert.toggle": "Alert me when an audit write fails",
  "audit.alert.title": "Audit write failed",

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

  // Chat panel (v0.8, 2/2).
  "chat.heading": "Chat",
  "chat.sessions": "Sessions ({count})",
  "chat.session_new": "New session",
  "chat.session_default_title": "New session",
  "chat.refresh": "Refresh",
  "chat.sessions_empty": "(no sessions yet)",
  "chat.rename_prompt": "Rename the session",
  "chat.rename": "Rename",
  "chat.delete_confirm": 'Delete the session "{title}"? This cannot be undone.',
  "chat.delete": "Delete",
  "chat.empty_hint":
    "Describe in plain language what you want the AI to do inside the sandbox, for example: “write a RISC-V bare-metal Hello World, compile and run it, and read the serial output back”.",
  "chat.tool": "Tool {name}",
  "chat.tool_running": "(running…)",
  "chat.tool_ok": "(ok)",
  "chat.tool_failed": "(failed)",
  "chat.thinking": "Thinking…",
  "chat.new_messages": "New messages ↓",
  "chat.input_placeholder": "Describe the task… (Enter sends, Shift+Enter adds a line)",
  "chat.running": "Running…",
  "chat.send": "Send",
  "chat.not_ready":
    "The model is not ready. Configure a provider and an API key in Settings.",
  "chat.toolchain_downloading": "Downloading the toolchain…",
  "chat.toolchain_missing":
    "RISC-V GCC was not found, so nothing can be compiled yet: use the one-click download in Settings → Toolchain.",

  // Serial canvas (v0.8, 2/2).
  "canvas.heading": "Serial canvas",
  "canvas.clear": "Clear",
  "canvas.export": "Export the serial log",
  "canvas.exported": "Exported {name} ({bytes} bytes)",
  "canvas.export_failed": "Export failed: {reason}",
  "canvas.jump_latest": "Jump to latest ↓",

  // Run list (v0.8, 2/2).
  "run.status.open": "in progress",
  "run.status.ok": "finished",
  "run.status.failed": "failed",
  "run.status.interrupted": "interrupted",
  "run.status.abandoned": "abandoned",
  "run.when.seconds": "{n} seconds ago",
  "run.when.minutes": "{n} minutes ago",
  "run.when.hours": "{n} hours ago",
  "run.when.days": "{n} days ago",
  "run.restored_from": "restored from {snapshot}",
  "run.empty":
    "No runs yet: after one run, runs with their configuration fingerprint appear here.",

  // Preflight wording (v0.8, 2/2).
  "preflight.step.gcc_runs": "the toolchain runs",
  "preflight.step.gcc_compiles": "compiling a minimal guest",
  "preflight.step.qemu_runs": "QEMU runs",
  "preflight.step.guest_boots": "the guest boots and echoes the banner",
  "preflight.state.ok": "passed",
  "preflight.state.failed": "failed",
  "preflight.state.not_run": "not run",
  "preflight.state.unchecked": "unchecked",
  "preflight.headline.unchecked":
    "Not checked yet: it runs by itself after you change the toolchain / QEMU path, and the button on the right runs it now.",
  "preflight.headline.ok":
    "Preflight passed: this environment really compiles and boots a guest.",
  "preflight.headline.overridden":
    "Preflight failed, but you chose to continue (the choice is recorded).",
  "preflight.headline.failed": "Preflight failed: stuck at “{step}”.",
  "preflight.progress.done_ok": "Preflight finished: everything passed.",
  "preflight.progress.done_failed": "Preflight finished: a step failed.",
  "preflight.progress.running": "Checking: {step}…",
  "preflight.progress.ok": "Passed: {step}",
  "preflight.progress.failed": "Failed: {step}",

  // Theme labels (v0.8, 2/2).
  "theme.light": "Light",
  "theme.dark": "Dark",
  "theme.system": "Follow the system",
  "theme.summary.system": "follows the system (currently {name})",
  "theme.summary.fixed": "fixed to {name}",

  // App chrome (v0.8, 2/2).
  "app.back": "← Back",
  "app.vm.running": "VM running",
  "app.vm.stopped": "VM stopped",
  "app.vm.badge_title": "VM {uptime} (kept across runs; stopped only when you ask)",
  "app.vm.uptime.seconds": "running {n} s",
  "app.vm.uptime.minutes": "running {n} min",
  "app.vm.uptime.hours": "running {n} h",
  "app.settings_title": "Settings (Esc closes)",

  // File pickers and run-export notes (v0.8, 2/2).
  "toolchain.pick_gcc_title": "Pick riscv64-unknown-elf-gcc",
  "toolchain.pick_qemu_title": "Pick qemu-system-riscv64",
  "audit.export_done": "Exported {lines} lines of {runId} to {target}",
  "audit.export_failed":
    "Could not export {runId}: {reason} (exports must stay inside the workspace)",

  // The Web client's front door and its first page (v0.9 D2b-2).
  "login.heading": "Sign in",
  "login.token_label": "Access token",
  "login.token_hint":
    "Read it from <data-dir>/token on the machine running the control plane. It is never shown here again.",
  "login.remember": "Remember this device",
  "login.submit": "Sign in",
  "login.unauthorized": "That token was refused. Check it and try again.",
  "login.unreachable": "The control plane did not answer. Is the server running, and at this address?",
  "login.other": "The server answered something unexpected. See the server's log.",
  "status.heading": "Status",
  "status.node": "This node",
  "status.counters": "Control plane",
  "status.version": "Version",
  "status.uptime": "Uptime",
  "status.uptime_seconds": "{n} s",
  "status.uptime_minutes": "{n} min",
  "status.uptime_hours": "{n} h",
  "status.connections": "Connections",
  "status.subscribers": "Event subscribers",
  "status.agents": "Agents",
  "status.agents_hint":
    "This node counts itself, and one is what a node with no executor roster reports (v0.9).",
  "status.agent_id": "Agent id",
  "status.refresh": "Refresh",
  "status.loading": "Reading…",
  "status.unreachable": "Could not read the node's status",

  // The browser is a read-only board (v0.9 D2b-4a).
  "web.readonly_note":
    "Read-only here: this board shows the node, and changes are made in the desktop app.",
  "web.appearance_note":
    "Theme and language are the two things this board can change — they are display preferences, not node settings.",

  // The node page's three tabs (v0.9 D2b-4b).
  "node.tab.status": "Status",
  "node.tab.executors": "Executors",
  "node.tab.sandboxes": "Sandboxes",
  "node.executors.count": "{count} executor(s) in this node's fleet",
  "node.executors.empty":
    "This node names no executor: its fleet is the `executors` block of settings.json.",
  "node.executors.note":
    "The node itself is not in this list; running on this node is the desktop's `run`.",
  "node.sandboxes.count": "{count} sandbox definition(s)",
  "node.sandboxes.empty": "No sandbox definition yet — they are written in the desktop app.",
  "node.sandboxes.current": "current",
  "node.sandboxes.default": "default",
  "node.sandboxes.source_manual": "manual",
  "node.sandboxes.source_discovered": "discovered",
  "node.sandboxes.runnable": "runnable",
  "node.sandboxes.not_runnable": "not runnable",
  "node.sandboxes.shadowed": "shadowed by a hand-written definition",
  "node.sandboxes.details": "Definition",
  "node.sandboxes.note":
    "Switching the node to another definition is a desktop control.",
  "node.candidates.heading": "Installed here ({count})",
  "node.candidates.empty": "The scan found nothing installed under the data directory.",
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
  "settings.tab.network": "网络",
  // 网络接入（v0.9.9）。同一页两个方向：「连出去」把本桌面端指向内网服务器，「服务进来」把本节点开放给网络。
  "network.out_heading": "连出去",
  "network.out_hint":
    "让本桌面端连上一台内网的 RiscDom 服务器，于是一块看板可以统管多个节点。连接本身在下一批实现。",
  "network.remote_url": "服务器地址",
  "network.remote_token": "令牌",
  "network.connect": "连接",
  "network.in_heading": "服务进来",
  "network.lan_enabled": "把本节点的看板开放到网络",
  "network.lan_bind": "绑定地址",
  "network.lan_bind_hint": "在上面的开关打开之前，只绑回环地址。",
  "network.allow_lan": "允许同网段的其它设备访问",
  "network.allow_lan_warning":
    "同网段的任何人都能访问本节点，唯一拦着他们的就是 token —— 请保管好它。",
  "network.lan_url": "看板地址",
  "network.lan_url_pending": "尚未启用",
  "network.lan_state_running": "正在服务",
  "network.firewall_hint":
    "Windows 首次可能弹窗问是否允许本应用访问网络 —— 请选「允许」，否则手机连不上。",
  "network.token_show": "显示 token",
  "network.token_copy": "复制",
  "network.token_path": "token 位于本节点的数据目录下的 <data-dir>/token。",
  "network.save": "保存",
  "network.saved": "已保存",
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
  "audit.alert.heading": "有审计写入失败，未能记录：",
  "audit.alert.toggle": "审计写入失败时提醒我",
  "audit.alert.title": "审计写入失败",

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

  // Chat panel (v0.8, 2/2).
  "chat.heading": "对话框",
  "chat.sessions": "会话 ({count})",
  "chat.session_new": "新建会话",
  "chat.session_default_title": "新会话",
  "chat.refresh": "刷新",
  "chat.sessions_empty": "（暂无会话）",
  "chat.rename_prompt": "重命名会话",
  "chat.rename": "改名",
  "chat.delete_confirm": "删除会话“{title}”？此操作不可撤销。",
  "chat.delete": "删除",
  "chat.empty_hint":
    "用自然语言描述你想让 AI 在沙箱里做什么，例如：“写一个 RISC-V 裸机 Hello World，编译运行并把串口输出读回来”。",
  "chat.tool": "工具 {name}",
  "chat.tool_running": "（运行中…）",
  "chat.tool_ok": "（成功）",
  "chat.tool_failed": "（失败）",
  "chat.thinking": "思考中…",
  "chat.new_messages": "有新消息 ↓",
  "chat.input_placeholder": "描述任务…（Enter 发送，Shift+Enter 换行）",
  "chat.running": "运行中…",
  "chat.send": "发送",
  "chat.not_ready": "模型未就绪，请在设置中配置服务商与 API Key。",
  "chat.toolchain_downloading": "正在下载工具链…",
  "chat.toolchain_missing":
    "未找到 RISC-V GCC，现在无法编译：请在「设置 → 工具链」中一键下载。",

  // Serial canvas (v0.8, 2/2).
  "canvas.heading": "串口画布",
  "canvas.clear": "清屏",
  "canvas.export": "导出串口日志",
  "canvas.exported": "已导出 {name}（{bytes} 字节）",
  "canvas.export_failed": "导出失败：{reason}",
  "canvas.jump_latest": "跳到最新 ↓",

  // Run list (v0.8, 2/2).
  "run.status.open": "进行中",
  "run.status.ok": "完成",
  "run.status.failed": "失败",
  "run.status.interrupted": "已中断",
  "run.status.abandoned": "已放弃",
  "run.when.seconds": "{n} 秒前",
  "run.when.minutes": "{n} 分钟前",
  "run.when.hours": "{n} 小时前",
  "run.when.days": "{n} 天前",
  "run.restored_from": "恢复自 {snapshot}",
  "run.empty": "暂无运行记录：跑一次之后，这里会出现带配置指纹的 run。",

  // Preflight wording (v0.8, 2/2).
  "preflight.step.gcc_runs": "工具链可运行",
  "preflight.step.gcc_compiles": "编译最小 guest",
  "preflight.step.qemu_runs": "QEMU 可运行",
  "preflight.step.guest_boots": "guest 启动并回显 banner",
  "preflight.state.ok": "通过",
  "preflight.state.failed": "失败",
  "preflight.state.not_run": "未跑",
  "preflight.state.unchecked": "未检查",
  "preflight.headline.unchecked":
    "尚未预检：改完工具链 / QEMU 路径后会自动跑一次；右侧按钮可随时手动触发。",
  "preflight.headline.ok": "预检通过：这套环境实际能编译并启动 guest。",
  "preflight.headline.overridden": "预检未通过，但你已选择继续（该选择已记录）。",
  "preflight.headline.failed": "预检未通过：卡在「{step}」。",
  "preflight.progress.done_ok": "预检完成：全部通过。",
  "preflight.progress.done_failed": "预检完成：有步骤未通过。",
  "preflight.progress.running": "正在检查：{step}…",
  "preflight.progress.ok": "已通过：{step}",
  "preflight.progress.failed": "未通过：{step}",

  // Theme labels (v0.8, 2/2).
  "theme.light": "浅色",
  "theme.dark": "深色",
  "theme.system": "跟随系统",
  "theme.summary.system": "跟随系统（当前{name}）",
  "theme.summary.fixed": "固定为{name}",

  // App chrome (v0.8, 2/2).
  "app.back": "← 返回",
  "app.vm.running": "VM 运行中",
  "app.vm.stopped": "VM 已停止",
  "app.vm.badge_title": "VM {uptime}（跨 run 保持；仅在你要求时停止）",
  "app.vm.uptime.seconds": "运行 {n} 秒",
  "app.vm.uptime.minutes": "运行 {n} 分钟",
  "app.vm.uptime.hours": "运行 {n} 小时",
  "app.settings_title": "设置（Esc 关闭）",

  // File pickers and run-export notes (v0.8, 2/2).
  "toolchain.pick_gcc_title": "选择 riscv64-unknown-elf-gcc",
  "toolchain.pick_qemu_title": "选择 qemu-system-riscv64",
  "audit.export_done": "已导出 {runId} 的 {lines} 行记录：{target}",
  "audit.export_failed": "导出 {runId} 失败：{reason}（只能导出到工作区内）",

  // 管理界面的入口与首屏（v0.9 D2b-2）。
  "login.heading": "登录",
  "login.token_label": "访问令牌",
  "login.token_hint":
    "在运行控制平面的机器上从 <data-dir>/token 读取。此处不会再次显示它。",
  "login.remember": "记住此设备",
  "login.submit": "登录",
  "login.unauthorized": "令牌被拒。请核对后重试。",
  "login.unreachable": "控制平面没有应答。服务在运行吗？地址对吗？",
  "login.other": "服务端返回了意外应答。请查看服务端日志。",
  "status.heading": "状态",
  "status.node": "本节点",
  "status.counters": "控制平面",
  "status.version": "版本",
  "status.uptime": "运行时长",
  "status.uptime_seconds": "{n} 秒",
  "status.uptime_minutes": "{n} 分钟",
  "status.uptime_hours": "{n} 小时",
  "status.connections": "连接数",
  "status.subscribers": "事件订阅者",
  "status.agents": "Agent 数",
  "status.agents_hint": "本节点把自己算作 1；尚未接入执行者名册的节点报的就是这个数（v0.9）。",
  "status.agent_id": "Agent 标识",
  "status.refresh": "刷新",
  "status.loading": "读取中……",
  "status.unreachable": "读不到节点状态",

  // 浏览器端是只读看板（v0.9 D2b-4a）。
  "web.readonly_note": "此处只读：本界面用于查看节点，修改请在桌面端进行。",
  "web.appearance_note":
    "主题与语言是本界面唯一能改的两项——它们属显示偏好，不是节点设置。",

  // 节点页的三个子 tab（v0.9 D2b-4b）。
  "node.tab.status": "状态",
  "node.tab.executors": "执行者",
  "node.tab.sandboxes": "沙箱",
  "node.executors.count": "本节点名册中有 {count} 个执行者",
  "node.executors.empty": "本节点未命名任何执行者：名册来自 settings.json 的 `executors`。",
  "node.executors.note": "本节点自己不在该列表中；要在本节点上跑，请用桌面端的 `run`。",
  "node.sandboxes.count": "{count} 个沙箱定义",
  "node.sandboxes.empty": "还没有沙箱定义——它们在桌面端编写。",
  "node.sandboxes.current": "当前",
  "node.sandboxes.default": "默认",
  "node.sandboxes.source_manual": "手写",
  "node.sandboxes.source_discovered": "扫描所得",
  "node.sandboxes.runnable": "可运行",
  "node.sandboxes.not_runnable": "不可运行",
  "node.sandboxes.shadowed": "被同名手写定义遮盖",
  "node.sandboxes.details": "定义",
  "node.sandboxes.note": "把节点切到另一个定义属桌面端控制操作。",
  "node.candidates.heading": "本机已装（{count}）",
  "node.candidates.empty": "扫描未在数据目录下发现任何已安装资源。",
};

/** The registry itself: language -> key -> template. */
export const STRINGS: Record<Language, Record<StringKey, string>> = { en: EN, zh: ZH };
