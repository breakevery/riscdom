# ui

智芯城（RiscDom）的桌面前端（Tauri 2 + React + TypeScript + Vite）。

## 三栏布局

- **左 · 对话框**（`panels/ChatPanel.tsx`）：消息列表（user / assistant / tool），
  底部输入框。工具调用显示为可折叠块（工具名 + 参数 + 结果）。运行中输入框禁用并显示"思考中…"。
- **中 · 设置**（`panels/SettingsPanel.tsx`）：LLM 配置、工作区、审计状态与最近事件。
- **右 · 串口画布**（`panels/CanvasPanel.tsx`）：xterm.js 终端，实时显示 guest 串口输出；
  顶部有 VM 状态条 + 清屏 + 导出串口日志。

三栏用 CSS Grid，中间两条可拖拽的分隔条（`layout/AppShell.tsx`，无第三方分栏库）。

## 运行

```powershell
npm install
npm run build        # tsc + vite build
npm run tauri dev    # 启动桌面应用（需要 Rust 工具链）
```

`npm run tauri build` 出安装包（需图标等，MVP 未打磨）。

## 设置 API Key（仅本次会话）

在**设置**栏填写 API Key / Base URL / Model，点"保存到本次会话"。

- Key 只存在后端内存（`host::AppState::llm_config`）；
- **不写 localStorage / sessionStorage / console / 审计 / 磁盘**；
- 前端保存后立即清空输入框；
- 状态显示只回显 `base_url` / `model`，**不回显 key**。

## 架构

```
前端 (React)  --invoke/listen-->  ui/src-tauri (Tauri shell)  -->  host crate
                                                                    |--> agent
                                                                    |--> sandbox
                                                                    |--> audit
```

- 所有 `invoke` 调用集中在 `src/api/tauri.ts`，便于审计。
- 前端**不直接**接触 QEMU / gcc / Rust crate。
- 不使用 `innerHTML` / `dangerouslySetInnerHTML` 渲染 LLM 或串口内容（防 XSS）。

## 事件

`agent:iteration` / `agent:tool_call` / `agent:tool_result` / `agent:final` /
`serial:chunk` / `vm:state`。详见 `host/README.md`。

## 测试

端到端（mock LLM，需 QEMU + RISC-V 工具链）：

```text
cargo test -p host -- --ignored --nocapture
```

真实 API 端到端（需 key）：

```powershell
$env:DEEPSEEK_API_KEY = "***"
cargo test -p agent -- --ignored --nocapture
npm run tauri dev   # 或直接手动操作界面
```

## 会话持久化

对话会自动保存，重启后可在 ChatPanel 顶部“会话”面板里打开 / 重命名 / 删除。

- 存储位置：**应用数据目录**下的 `sessions.db`（SQLite），不在仓库、不在 AI 工作区。
- 可用环境变量 `RISCDOM_SESSION_DB_PATH` 覆盖路径。
- 清除：在会话面板逐条删除，或调用 `clear_all_sessions`（UI 二次确认）。
- **不会**持久化：API Key、system prompt 原文、流式中间状态、审计事件。
- 恢复会话只把历史消息注入 `AgentLoop`，**不重放**工具调用。

## v0.2 待办

- keyring 持久化（系统钥匙串，替代仅会话内）
- 流式输出（SSE 逐字）
- 会话：搜索 / 标签 / 导入导出 / 加密（均属 v0.2 后续）
- 串口改为 sandbox 主动回调（去掉 host 轮询）
