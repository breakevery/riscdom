[English](manual-acceptance.md) | 中文

# 人工验收清单

> **这份文件为什么存在。** v0.9.0 带着一个没有任何自动化发现的 P0 发出去了：桌面版打开就停在一个
> 怎么输都过不去的登录页。所有测试都是绿的，因为自动化检查跑在 Node 里，那里「我是不是桌面端」的答案
> 永远是**否** —— 桌面分支从未被任何东西执行过。本仓所依赖的那次走查，一直只活在人的脑子里。这份文件
> 就是那次走查，落到盘上。

**它不是黄金路径表单。** [golden-path-checklist.md](golden-path-checklist.zh-CN.md) 是走查者为黄金路径
第 1–7 步填写的那份记录（机器、版本、指纹、导出的记录）。本文件是**围绕它的工作顺序**：装、起、驱动、
核对，以及出问题时该回传什么。走一次发布，两份都填。

**谁来走。** 写代码之外的人，在一台从未装过 RiscDom 的机器上。

## 准备

- 一台**干净机器**（或一台没装过 RiscDom 的机器），最好每个平台各一台。
- 你自己的**模型 API key**（或本地提供方，如 Ollama / LM Studio）。
- **由你安装的 QEMU**：`qemu-system-riscv64`（Windows：`winget install
  SoftwareFreedomConservancy.QEMU`；macOS：`brew install qemu`；Linux：发行版自己的包）。按决策（§7），
  RiscDom **从不**打包或下载 QEMU。
- 一台**同一局域网里的手机**（用于层次 6）。
- 能写字的地方：填好的 [golden-path-checklist.md](golden-path-checklist.zh-CN.md)，以及任何失败记录。

## 层次 1 —— 装

从发布页取你平台的包并安装。**所有包都未签名**，所以下面这些是预期行为，不是 bug：

| 平台 | 包 | 系统会怎么做 |
|---|---|---|
| Windows 10/11 | `RiscDom_<版本>_x64_en-US.msi` 或 `RiscDom_<版本>_x64-setup.exe` | SmartScreen 告警：*更多信息 → 仍要运行* |
| macOS（Apple Silicon） | `RiscDom_<版本>_aarch64.dmg` | Gatekeeper 拦下首次启动：右键 App → *打开*，或执行一次 `xattr -dr com.apple.quarantine /Applications/RiscDom.app` |
| Linux（amd64） | `RiscDom_<版本>_amd64.deb`、`RiscDom-<版本>-1.x86_64.rpm` 或 `RiscDom_<版本>_amd64.AppImage` | 用包管理器装，或就地运行 |

- [ ] 包能装，应用出现（菜单 / 应用列表 / 路径里的 `riscdom`）。
- [ ] 卸载能干净移除。若留下什么，写下来。

## 层次 2 —— 起

**这正是 v0.9.0 断掉的层次。** 打开应用。

- [ ] **直接是主界面 —— 不是登录页。** 若你看到一个要 token 的提示，你就找到了一个阻塞项：桌面端自己
      就跑着那个节点，它没有可登录的控制平面（P0）。
- [ ] 外壳显示聊天面板与串口面板。
- [ ] 右上齿轮能打开*设置*，且**六个 tab 全部可切**：模型 / 工具链 / 快照 / 审计 / 插件 / 外观。
- [ ] *设置 → 外观*：切**语言**（跟随系统 / English / 中文）立即生效，切**主题**
      （浅色 / 深色 / 跟随系统）也立即生效。
- [ ] 关掉再打开：语言与主题还在（它们写在 `<workspace>/.riscdom/settings.json`）。

## 层次 3 —— 黄金路径

完整记录填进 [golden-path-checklist.md](golden-path-checklist.zh-CN.md)；这里只是操作。

- [ ] *设置 → 模型*：选一个提供方，填 **API key**（勾选后存入 OS 钥匙串）、Base URL 与模型。状态行
      显示配置已生效。
- [ ] *设置 → 工具链*：**RISC-V GCC** 被找到或装上 —— 应用内下载只有一个按钮；手动选路径也行。
- [ ] *设置 → 工具链*：**QEMU** 探测找到 `qemu-system-riscv64`。
- [ ] *设置 → 工具链 → 环境预检*：跑一遍。各步全过，或失败时点名是哪一步、并可用理由覆盖。
- [ ] 在聊天框里要一件端到端的事：*「写一个 RISC-V 裸机 hello，编译、运行，并把 hello 从串口打出来。」*
      智能体会编译、启动客机、把串口输出读回来。
- [ ] **串口面板打出客机的 `hello`**（终端随运行填充）。
- [ ] *设置 → 审计* 列出这次运行的事件。

## 层次 4 —— 审计链

- [ ] *设置 → 审计*：事件列表加载出来（新→旧），表头说链是完整的。
- [ ] 导出并检查文件：`riscdom export audit-jsonl --out audit.jsonl`，或按运行
      `riscdom export run-audit <run_id>`（路径相对 workspace 解析）。
- [ ] 独立检查器同意：`audit-verify <workspace>/.riscdom/audit.db --runs` →
      `Intact { length: N }` 且 `RunIndex { findings: 0 }`。
- [ ] 现在破坏一个**副本**：把 `audit.db` 复制到别处，在副本里执行
      `DROP TRIGGER audit_no_update; UPDATE audit_events SET action = 'evil' WHERE id = 2;`
      （真文件上触发器会拒绝这么干 —— 那正是它们存在的意义）。
      `audit-verify <副本> --runs` → **`Broken`**，并点名那个 id。真文件未被动过。

## 层次 5 —— 命令行

- [ ] 在一个控制台里起控制平面：`riscdom-server --workspace <dir> --log-level info`。它会打印绑定的
      地址，并在首次启动时**生成 `<data-dir>/token`**。
- [ ] 在另一个控制台：`riscdom --remote 127.0.0.1:7821 health` → 节点的状态行。
- [ ] `riscdom --remote 127.0.0.1:7821 status` → 连接数、订阅数、agents、`agent_id`。
- [ ] `riscdom --remote 127.0.0.1:7821 sandboxes list` → 合并后的注册表，带 `current` 与 `default`。
- [ ] **错的 token 会被拒**：`riscdom --remote 127.0.0.1:7821 --token wrong health` 退出码为 **`4`**
      （并打印控制平面自己的拒绝语，而不是猜一个）。
- [ ] `riscdom health --json` 原样透传 JSON。

## 层次 6 —— 手机上的看板

- [ ] 在局域网上提供构建好的前端：`riscdom-server --workspace <dir> --bind 0.0.0.0:7821
      --web-root ui/dist/app`（构建是 `ui/` 下 `npm run build`，它写进 `ui/dist/app/`；服务端把 `index.html` 挂在 `/`、hash 资源
      挂在 `/assets/*`）。
- [ ] 在手机上打开 `http://<本机局域网地址>:7821`。出现**登录门**。
- [ ] 用 `<data-dir>/token` 里的 token 登录（那个文件也在这台机器上）。节点页打开，带三个子 tab：
      **Status / Executors / Sandboxes**。
- [ ] 在手机看着的时候，在桌面应用上跑点东西：几秒内看板会自己刷新（事件流）。
- [ ] 看板是**只读**的：属于桌面的控制都不在，只有两个显示偏好（主题与语言）例外，它们可用。

## 层次 7 —— 另外两种语言（可选，加分）

- [ ] *设置 → 工具链*：装 **Zig**，然后让智能体写一个能启动的 Zig hello。
- [ ] *设置 → 工具链*：装 **Rust** sysroot，然后要一个能启动的 Rust hello。

## 层次 8 —— 网络上的看板（v0.9.9）

- [ ] *设置 → 网络*：打开「把本节点的看板开放到网络」，再打开「允许同网段的其它设备访问」（警示会出现——读一遍，并把 token 自己收好）。
- [ ] 保存。**Windows 可能会问是否允许本应用使用网络——请选「允许」**，否则局域网上的任何东西都到不了它。macOS 可能由它自己的防火墙来问；Linux 在这里没有提示。
- [ ] 页面现在会显示**看板地址**（`http://<本机地址>:7821`）与状态「正在服务」，并在括号里给出绑定地址（`0.0.0.0:7821`）。
- [ ] 在**同一局域网的手机**上打开那个地址。**登录门**出现；用「显示 token → 复制」拿到 token 后登录。节点页带三个子 tab 打开。
- [ ] 在手机看着的时候，在桌面端跑点东西：看板会自己刷新。
- [ ] 把看板**关掉**并保存：手机侧的连接会在几秒内失效。
- [ ] 关掉应用再打开：看板会自己回来（设置记得它）——若你关了它，那就保持关闭。

## 通过标准

一次发布要走查成功，**层次 1–7 必须全过**。**层次 8 是最新的一层**，而只有手机能验它——有手机时走一遍，但缺了它不算这次发布失败。

## 如何反馈

在 <https://github.com/breakevery/riscdom/issues> 开 issue，或把填好的
[golden-path-checklist.md](golden-path-checklist.zh-CN.md) 放进 `walkthroughs/`。每个问题都给：

- **层次**（1–7）与**平台**（系统 + 构建号、用的是哪个包）；
- **你做了什么** —— 能再现它的最短步骤；
- **发生了什么** —— 逐字抄下那句消息；若屏幕就是证据，附截图；
- **你预期什么**；
- 日志里的任何东西：服务端打印的控制台，或审计导出。

## 优先级

| 优先级 | 含义 |
|---|---|
| **P0** | 层次 1–3 失败：装不上、打不开、驱动不了。这次发布不可用。 |
| **P1** | 层次 4–6 失败：应用能用，但审计、CLI 或局域网看板不行。 |
| **P2** | 层次 7 失败，或某个已记载的行为与文档不符。 |
| **P3** | 任何外观问题：措辞、间距、提示文字。 |
