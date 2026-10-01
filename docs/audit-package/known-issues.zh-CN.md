[English](known-issues.md) | 中文

# RiscDom —— 已知问题、未完成项、与未经真机验证之处

> **这些从哪来。** 本页没有一条是临时意见。每条都取自项目自有记录 ——
> [handoff.md](../handoff.md) §1、[roadmap-v1.0.md](../roadmap-v1.0.md) §12 与 §15、
> [cross-chain-verification.md](../cross-chain-verification.md) §6–§7，以及
> [README.md](../../README.md) 的已知局限。凡是拿不准的，本页标为不确定，而不粉饰。版本为
> `v1.0.0`。

## 1. 未完成

- **M8-4 未完成。** 控制平面已在 **M8-4a** 拆到它自己的仓（`riscdom-server`），但还余三批：
  **M8-4b**（桌面仓）、**M8-4c**（本仓收尾 —— 删 `server/`、把打包脚本裁成只打 relay、改 CI 作业
  名、文档改指向）、**M8-4d**（三个仓 lock 文件的对账）。今天的后果：**`server/` 仍是本仓的一个
  crate**，因为 `ui/src-tauri` 以 path 依赖它，而 `ui/` 要到 M8-4b 才离开。
- **M 的跨区域层推迟到 v1.x**（roadmap §13；decisions §157）。`examples/` 下的参考 M 只读一个
  **工作组**（`--level lan`）；其上一层 —— 一组 LAN 级 M 加一个 `--config` 文件 —— 不在 v1.0 范围。
- **`--follow` / `--wait` 的毛边**（roadmap §12）：进程退出时流可能还开着。`--follow` 会在运行
  结束时终止，但这条缝没有闭合。
- **关键事件推送没有读者**（cross-chain §7）：自 M4e-2 起，链分叉会*同时*作为关键事件推给服务端，
  好让聚合角色立刻听到 —— 而**没有任何路由或命令读这份日志**。今天这次推送只到服务端内存，部署者
  唯一的可见面是节点自己的链。记录为已知缺口，明确**不是**一条能工作的通知路径。
- **跨链刻意缺席的部分**（cross-chain §2、§6）：**没有冲突规则**、中心自己的摘要链上**没有范围
  证明**（每条变更是写一行，不是第二条链）、并且**没有任何端点列出分段** —— 分叉像其它事实一样，
  从链里读出。`ChainDigest` 仍是对一个头部的承诺。
- **可观测性只有规格**（handoff §1，M7g / decisions §115）：[observability.md](../observability.md)
  写下了结构化日志字段与 `/metrics` 路由，但**写入方与路由都留待以后** —— 路由在其第一个消费者出现
  时才落地。`v1.0.0` 处没有任何 metrics 端点。
- **存活性协议不定义移除**（connection.md §6.7）：一次裁断记录 `judged_at_ms` 并写
  `peer_offline` / `peer_recovered`，但**踢人是部署者的事** —— 协议本身没有移除步骤。
- **Python 作为访客语言缺席**（roadmap §12）：现有 C、Zig 与 Rust。
- **会话库的 WAL 模式刻意未设**（decisions §54），v1.0 未变。
- **GUI 开关的点击区**（roadmap §12）：开关是一个约 14 px 的裸复选框；计划用整行可点的 `Toggle`
  替换，仍未做。

## 2. 已知 flaky

- **QEMU 与 gate 的并行 flake** 是项目最老的未结项（roadmap §12）：QMP `10054` 家族与端口竞争
  挂起。它们**每次都上报、从不掩盖** —— 所以本页也照说，而不藏。
- **在 [test-evidence.md](test-evidence.md) 背后的那次 gate 运行中**，`net` 的 relay 测试
  `a_frame_reaches_the_destination_through_the_server` 报了 `got Direct`：帧走了**直连**路径，而
  测试期望 relay。这是夹具里的时序/端口竞争，不是功能变化 —— 与上条同族。

## 3. 未经真机验证

- **macOS 与 Linux 包由 CI 构建，但未签名、也从未被走过。** README 的已知局限说得直白：Unix socket
  上的 QMP **仍未实现**（只有 TCP）。golden path 只在 **Windows** 上验证过。
- **v1.0 没做干净机器的走查。** `manual-acceptance.md` 要求一个没写代码的人、在一台从没装过
  RiscDom 的机器上走一遍；那次走查没有记录。
- **第 8 层（手机看板）与第 9 层（桌面连另一节点）没有手机与第二节点就走不了。**
  `manual-acceptance.md` 的通过标准说第 1–7 层必须过，把 8–9 当新增。
- **「第一次重启后本地看板没了」** —— v0.9.9 走查里一个未解释的观察：首次重启后起在了本地看板，
  远端地址不见了。清掉陈旧的开发者进程后未能复现，且**没有代码路径能解释它**。第 9 层才是结它的
  地方。
- **`v1.0.0` tag 本身**是在 Windows 上打的；发布资产（安装包、server 压缩包）由 CI 产出，但只有
  Windows 的走查有记录。

## 4. 技术债

- **三条 CLI 端到端测试以降低的覆盖运行**（v1.0 批 DT / M8-4a）。CLI 变成纯客户端、不再自启任何
  服务，于是 `cli/tests/control.rs` 的端到端测试需要一个真的 `riscdom-server` 二进制；当测试二进制
  旁边没有它（也没通过 `RISCDOM_SERVER_BIN` 给出）时，**它们跳过**而非失败。**M8-4d 才是跨仓集成
  回归的地方。** 在已构建过 workspace 的机器上 —— 如本机为 [test-evidence.md](test-evidence.md)
  所做 —— 它们照常运行。
- **`server/` 仍在核心仓里**直到 M8-4c（见 §1）：这里一个消费者（`ui/src-tauri`）比该 crate 活得
  更久，所以 crate 不能先走。
- **一行陈旧的文档**：`docs/README.md` 的 `RELEASE_NOTES.md` 行仍把「v0.9.9」写成最新发布，而
  `RELEASE_NOTES.md` 已整篇换成 v1.0。记在此处，而非悄悄改掉。
- **桌面仓（`riscdom-adminapp`）尚不存在**，且 `multi-repo.md` §2 的示例（管理程序*在内核的 tag*
  取 `server`）在 `server` 离开内核后已不成立；修正后的规则是 adminapp 钉 **`riscdom-server` 自己
  的 tag**。

## 5. 什么*不算*问题

为平衡，也免得读者把开放当成故障：

- **审计链的核心** —— `compute_hash`、`verify_chain` 与只追加触发器 —— 整条 v1.0 线上未动。
- **API 冻结**是刻意的边界，不是缺失的功能：`/v0/` 是 v1.0 的路径前缀，变更受
  [api-compatibility.md](../api-compatibility.md) 约束。
- 上面那些在 roadmap 里标 `[open]` 的条目是**刻意**开着的 —— 记下来，好让日后的讨论不必重新发现
  它们。
