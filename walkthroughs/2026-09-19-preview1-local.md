# 黄金路径走查记录 — v0.5.0-preview.1 — 2026-09-19

> **来源**：本记录来自**开发机（非全新环境）**，用**真实模型 + 真实 API key + 安装版 MSI** 走完黄金路径
> 七步，全部通过；外部测试者的记录待补（`walkthroughs/README.md` 说明了本目录的用途）。

> **注意：这不是「陌生开发者的干净机器首跑」。** 本机是开发机，QEMU 与 RISC-V GCC 早已安装；
> 走查由 AI 测试者用「安装版 MSI」在本机执行。模板中无法在本机验证的项已逐条注明，未编造。

**机器与日期**
- 机器：Z0624145651262（x64），8 逻辑核，16 GB 内存
- 日期：2026-09-19 — 操作人：AI 测试者（本会话），用真实模型、真实 API key

**操作系统**
- Windows 11 企业版 — build 22631

**QEMU（第 1 步）**
- `qemu-system-riscv64 --version` → QEMU emulator version 11.1.0 (v11.1.0-12130-ge470268ff4)
- 来源：**无法验证**（本机早已安装，非本次走查所装）
- 「设置 → 工具链」里的路径：`C:\Program Files\qemu\qemu-system-riscv64.exe`（显示为 KnownPath，自动探测到）

**RISC-V GCC（第 1 步）**
- `riscv64-unknown-elf-gcc --version` → xPack GNU RISC-V Embedded GCC x86_64 15.2.0
- 来源：**无法验证**（本机早已安装；另有 `D:\tools\xpack-riscv-none-elf-gcc-15.2.0-1`）
- 「设置 → 工具链」里的路径：`D:\tools\riscv64-unknown-elf\bin\riscv64-unknown-elf-gcc.exe`（KnownPath；第 7 步时改为 Manual）

**服务商与模型（第 2 步）**
- 服务商：DeepSeek — 模型：deepseek-chat（实际用于全部 5 次 run）
- Base URL：https://api.deepseek.com
- key：由环境变量 `DEEPSEEK_API_KEY` 提供（未打印任何片段）；走查中也在「设置 → 模型」里粘贴过一次，
  默认勾选了「保存到系统钥匙串」，于是写入 Windows 凭据管理器；**走查结束时用「清除」按钮清掉了钥匙串条目**
  （界面回显「已清除（含系统钥匙串条目）」「未配置（API key 不会落盘）」）
- 断言：走查后在 app 数据目录、我的临时目录、安装包目录中搜索 key 字面量 → **未出现在任何文件中**

**预检（第 2 步 — 设置 → 工具链 → 环境预检）**
- 第 1 步 工具链可运行：通过
- 第 2 步 编译最小 guest：通过
- 第 3 步 QEMU 可运行：通过
- 第 4 步 guest 启动并回显 banner：通过
- 结论：**全部通过**（绿点：「预检通过：这套环境实际能编译并启动 guest。」）

**安装（第 2 步之前）**
- 用 **MSI**：`RiscDom_0.5.0-preview.1_x64_en-US.msi`，`msiexec /qn` 静默安装成功（本进程为管理员）
- **未观察到 SmartScreen 拦截**（静默安装不经过浏览器下载的 SmartScreen 判定）；NSIS `.exe` 路径未测
- 安装位置：`C:\Program Files\RiscDom\`（`ui.exe`）
- **「应用和功能」里显示的版本是 `0.5.0.1`**，不是 `0.5.0-preview.1`（见问题清单 G-3）

**两次 run 的短指纹**
- 变更前（多次 run 相同）：`3e6b7ae97dc2aeea`
- 变更后：`95925841a5e50419`
- 两者不同：**是** — 变更的是 `toolchain.source`（KnownPath → Manual，指向同一个编译器）
- 并排面板：审计页勾选两个 run 后并排显示「短指纹 + 全量指纹 + 开始时间 + 状态」，
  左侧 `95925841a5e50419 …`（16:04:14，完成）与右侧 `3e6b7ae97dc2aeea …`（16:02:35，完成）**并排可见**

**导出的审计记录**
- 文件：`%APPDATA%\com.breakevery.riscdom\workspace\run_01a0b8a5-acef-73b6-974e-1e43cb339536.audit.jsonl`（16,962 字节）
- 默认路径由应用给出（工作区根 + `<run_id>.audit.jsonl`），在原生「另存为」里直接确认
- 用 Python 把导出文件写进**空**数据库后再验：首行 `prev_hash` 为 genesis → `audit-verify` → `Intact { length: 42 }`（退出码 0）
- `audit-rebuild` → `IndexRebuilt { runs: 1, starts: 1, ends: 1, abandoned: 0, orphans: 0 }` + `RunIndex { findings: 0 }`（退出码 0）
- `audit-verify --runs` → `RunIndex { findings: 0 }` + `Intact { length: 42 }`（退出码 0）

**七步逐步结果**

1. 创建环境（第 1–2 步）：**成功** — 自动探测到 GCC（KnownPath）与 QEMU（KnownPath）；预检四步全通过。
2. 预检失败时的处理：**未验证** — 未构造失败环境（本机工具链完好）。
3. 跑 Agent 任务（真实模型）：**成功** — 5 次迭代：write_source → compile → start_vm → read_serial → 总结；
   串口画布收到 `Hello, RISC-V bare metal! / RiscDom sandbox: RV64GC @ QEMU virt`；审计链 42 条、完整。
4. 保存 snapshot：**成功** — 弹窗命名（默认值 `snap1`，直接确认）→ `snap1.mig` 484 KB，
   快照页显示「1 个快照 / 真实 / snap1 484 KB」。
5. 导出审计记录 + 校验：**成功** — 见上（另存为对话框默认落在工作区，文件名即 run id）。
6. 回滚：**成功** — 点「恢复」后 QEMU 重新启动，审计库里出现**它自己的 run**（`parent_run_id` = 任务 run、
   `resumed_from_snapshot` = `snap1`、`run.end` 的 reason = `snapshot restored`）。
7. 换配置重跑 + 并排指纹：**成功** — 改了工具链来源（KnownPath → Manual）后重跑，新 run 指纹不同，
   并在审计页并排面板里可见（见上）。

**失败记录**
- 出在哪一步：第 7 步（改配置）**第一次尝试失败** — 在「设置 → 模型」里把 Model 改成 `deepseek-reasoner`
  并点「保存到本次会话」后，随后的 run 仍以 `deepseek-chat` 发出（审计事件 `agent.llm.request.model` 为证）。
- 原始现象：界面上表单里的 Model 值确实变新了，但保存后状态行没有变化，run 使用的模型也没变。
- 是否可恢复：**是** — 改为修改「工具链来源」（点「手动输入」并在弹窗里确认已探测到的同一路径），
  指纹随之改变、run 正常。
- 说明：这次失败也可能是自动化点击没命中「保存到本次会话」按钮；**需要真人用鼠标复核一次**（见问题清单 G-2）。

**问题清单（建议各自开一个 issue，本轮不建、不修）**

- **阻断（仅对自动化测试者）**：无（对整个黄金路径没有出现阻断性缺陷）。
- **严重**
  - S-1 审计页的 run 行在默认/最大化窗口宽度下**几乎完全被挤出可视区**：只看到一个复选框和一条横向滚动条，
    需要横向拖动滚动条才看得到状态/指纹/时间/「导出」按钮；拖到底后文字又变成**逐字竖排**。
    后果：第 5 步（导出）在真实操作里很容易找不到入口。
- **一般**
  - G-1 快照命名弹窗的**默认值写死为 `snap1`**，用户直接回车会得到同名快照；建议留空或给带日期的默认值。
  - G-2 「设置 → 模型」改 Model 后点「保存到本次会话」，本次走查**未能让模型生效**（见上）。需要真人复核：
    是保存按钮未命中，还是保存静默无效果（例如 key 输入框为空时不保存）。
  - G-3 安装后「应用和功能」显示的版本号是 **`0.5.0.1`**（`bundle.windows.wix.version` 的值），
    而产物名/包版本是 `0.5.0-preview.1`。测试者看到的版本号与下载到的版本名不一致。
  - G-4 QEMU 的控制台窗口会以**可见窗口**出现并抢占前台（回滚后尤其明显，一度盖住应用）；
    对操作者是干扰，也让「哪个窗口是应用」变得不清楚。
- **体验**
  - E-1 对话里的工具调用条显示成 `?? write_source ?`（问号标记），看不出调用/结果状态。
  - E-2 预检段落里的「也可以手动点「重新预检」」这句中的「重新预检」看起来可点，实际不可点；
    真正可点的按钮在句子右侧（本次走查先点错了一次）。
  - E-3 审计页的 actor 过滤器在失焦时才生效（文档已说明），但在输入过程中看不出「还没生效」。

**与 MockLlm 的差异（真实模型暴露、mock 测不出的点）**

1. 真实模型写出的 C 与夹具不同（688 字节 vs 夹具 ~1005 字节，自己的 UART 轮询写法），
   串口文本也不同（`Hello World` vs 夹具的 `HELLO RISCV`）—— 任何断言「串口必须是某段固定文本」的 mock 测试都覆盖不到。
2. 第二次运行时 VM 已在运行，模型 `start_vm` 收到工具错误「a VM is already running; reuse it」，
   **模型自己改为复用现有 VM 并直接 read_serial** —— 这是 mock 脚本不会走的分支（工具错误后的自我纠偏）。
3. 真实模型每次 5 轮迭代、每次响应都是自然语言+工具调用；mock 是固定脚本，迭代次数与措辞都不真实。
4. 真实延迟显著（每次 run 约 40–90 秒，含编译与启动 guest），mock 是秒级 —— 依赖「run 很快结束」的假设不成立。
5. 真实模型会主动在总结里引用它读到的串口内容 —— 说明「读串口」这一步的返回确实进了上下文。

**未完成或无法验证的项 + 原因**

1. QEMU / GCC 的**来源**（winget / 官方安装包 / 手动 / 应用内下载）：本机早已装好，无法回溯。
2. **不是全新机器**：本机已装 QEMU、GCC，也曾跑过本项目的测试；因此「干净环境首跑」这件事本记录不能替代。
3. 预检**失败**路径：未构造失败环境。
4. **中文输入**：经由我使用的合成输入，中文进到聊天框会变成乱码（英文与 ASCII 正常）。
   我判断这是**自动化注入与输入法的交互**，不是应用缺陷；但需要真人用真实键盘确认一次中文输入正常。
5. 「保存模型」是否真的无效（G-2）：需要真人复核。
6. macOS / Linux：未支持、未验证。
7. NSIS `.exe` 安装路径（含 SmartScreen 提示）未测；只用 MSI 静默安装。
8. 安装包**代码签名**：未签名（预期）；未在本机做交互式安装体验（UAC/SmartScreen 观感）。

**环境恢复**
- 走查前 `%APPDATA%\com.breakevery.riscdom` **不存在**，因此无备份可恢复；走查**新建**了它，内容为：
  `sessions.db`、`workspace\{hello.c,hello.elf,run_…audit.jsonl}`、`workspace\.riscdom\{audit.db,preflight\,snapshots\snap1.mig}`。
  这些是应用自己的数据（其中不含 key 明文），保留原样以便复核。
- 应用已退出；走查期间启动的 QEMU 进程已全部结束；钥匙串条目已清除；环境变量 `DEEPSEEK_API_KEY` **仍在**（由用户撤销）。
- **RiscDom 仍安装在本机**（`C:\Program Files\RiscDom\ui.exe`），未卸载。
