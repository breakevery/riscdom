[English](demo.md) | 中文

# RiscDom —— 30 分钟演示

> **这是什么。** 七步展示整个产品：安装、配置模型、跑任务、读审计链、快照、回滚、跨节点派发。
> 每步给出**目的**、**命令**、**预期输出**，以及一个可核对的**验收点**。两个脚本驱动其中能驱动
> 的部分：[`scripts/demo.ps1`](../../scripts/demo.ps1)（Windows）与
> [`scripts/demo.sh`](../../scripts/demo.sh)（Unix）。
>
> **脚本是半自动的，这是刻意的。** 这次走查里有三件事无法诚实地脚本化：**你的模型 API key**
> （一个秘密）、**第二个节点的配置**（一个关于你网络的决定）、以及**访客是否真的打印了 `hello`**
> （要人读串口面板）。脚本会真跑每一步自动部分 —— 构建、起一个控制平面、调用只读与审计面、保存/
> 恢复一次快照 —— 并对每个需要你动手的步骤**打印确切命令**。绝不为让屏幕变绿而伪造任何一步。

## 开始之前

- **QEMU**（`qemu-system-riscv64`）与 **RISC-V 裸机 GCC** 在 `PATH` 上，加上 Rust 工具链。一条命令
  的自检是第 1 步。
- **一个可指向的控制平面**。自 v1.0 起 CLI 是**纯客户端** —— `--remote` 必需，CLI 不自启任何东西。
  脚本会替你把 `riscdom-server` 起来。
- **一个模型。** 要么是托管提供方的 API key，要么是本地模型（Ollama / LM Studio）以完全离线走查。

全程脚本使用仓内 `target/` 下的临时 workspace 与 data 目录，所以演示绝不碰你真正的
`settings.json` 或 `audit.db`。

---

## 第 1 步 —— 安装 / 校验环境

**目的。** 证明这台机器根本跑得起来。

**命令。** 先构建，再在回环端口起一个控制平面并读其活性：

```sh
cargo build --workspace
riscdom-server --bind 127.0.0.1:7821 --workspace target/demo-ws --data-dir target/demo-data
# 另开一个控制台：
riscdom --remote 127.0.0.1:7821 health
```

**预期输出。** 服务端打印它绑定的地址，并在首次启动时写出 `<data-dir>/token`；`health` 打印
节点状态行。

**验收点。** `health` 有应答；且 `riscdom --remote 127.0.0.1:7821 --token wrong health` 退出码为
**`4`** —— 错 token 被服务端自己的消息拒绝，而不是一句猜测。

**自动？** 是 —— 脚本会构建、起服务端、检查退出码。

---

## 第 2 步 —— 配置模型（BYOK）

**目的。** 把节点指向一个模型。本项目不提供、不托管、不内嵌任何 key。

**命令。**

```sh
export DEEPSEEK_API_KEY="sk-..."
riscdom --remote 127.0.0.1:7821 llm set --provider deepseek --model deepseek-chat
# 或本地模型：
#   riscdom --remote 127.0.0.1:7821 llm set --provider ollama --base-url http://127.0.0.1:11434
```

**预期输出。** 节点报告配置已生效；`GET /v0/status` 显示有一个已配置的 agent。

**验收点。** 在**未**配置模型时，`riscdom run "say hi"` 干净地以控制平面的 `unavailable` 与退出码
**`3`** 失败 —— 节点宁拒绝，不猜。

**自动？** **否** —— key 是你的。脚本打印命令并等待；离线路径（Ollama）无需 key。

---

## 第 3 步 —— 端到端跑一条任务

**目的。** 一条命令走完整个环：模型写代码、为 RISC-V 编译、在 QEMU 里启动、读回串口。

**命令。**

```sh
riscdom --remote 127.0.0.1:7821 run \
  "Write a RISC-V bare-metal hello, compile it, run it, and print hello over the serial console."
```

**预期输出。** agent 循环运行；编译器产出 ELF；QEMU 启动它；**串口面板打印访客的 `hello`**。
随后 `riscdom runs list` 显示该运行。

**验收点。** 串口输出包含任务要求的那个词，且该运行出现在 `GET /v0/runs`。

**自动？** **部分。** 有 key 时脚本会跑这条命令；**你**确认访客的输出。

---

## 第 4 步 —— 读审计链

**目的。** 看见每个动作都被记录，且记录可验证。

**命令。**

```sh
riscdom --remote 127.0.0.1:7821 audit status
riscdom --remote 127.0.0.1:7821 audit events --limit 20
riscdom --remote 127.0.0.1:7821 export audit-jsonl --out target/demo-audit.jsonl
audit-verify target/demo-data/audit.db --runs
```

**预期输出。** `audit status` 报事件数与链的裁决；`audit events` 最新在前地列出；`audit-verify`
打印 `Intact { length: N }` 与 `RunIndex { findings: 0 }`。

**验收点。** 独立校验器（一个*独立*二进制，不是运行中的应用）同意链完好。然后打破一份**拷贝**：

```sh
cp target/demo-data/audit.db target/demo-audit-copy.db
# 只在拷贝里：
#   DROP TRIGGER audit_no_update; UPDATE audit_events SET action = 'evil' WHERE id = 2;
audit-verify target/demo-audit-copy.db --runs     # -> Broken，并点名那个 id
```

**真的**那个文件仍然验为 `Intact` —— 触发器在它上面拒绝这次改动。这就是只追加的性质，被展示而非
被断言。

**自动？** 是 —— 脚本跑这些读、导出与校验器，并执行「打破一份拷贝」的实验。

---

## 第 5 步 —— 快照

**目的。** 捕获正在运行的访客的状态。

**命令。**

```sh
riscdom --remote 127.0.0.1:7821 snapshots save demo-before
riscdom --remote 127.0.0.1:7821 snapshots list
```

**预期输出。** 名为 `demo-before` 的快照出现在列表里。

**验收点。** **未**在运行时保存会是一个显式错误，而非静默 no-op —— 所以这一步也证明了 VM 真的起
着。

**自动？** 是 —— 脚本保存并列出。

---

## 第 6 步 —— 回滚

**目的。** 把访客恢复到快照。

**命令。**

```sh
riscdom --remote 127.0.0.1:7821 snapshots resume demo-before
```

**预期输出。** 访客被恢复；串口继续。

**验收点。** 恢复会点名它用的那个快照，且该运行的历史仍可读。

**自动？** 是 —— 脚本执行恢复。

---

## 第 7 步 —— 跨节点派发

**目的。** 展示第三层：一条任务跑在**另一个节点**上。

**命令。** 用**自己的** data 目录起第二个节点（同一目录意味着同一 token，检查会平凡地通过），
然后派发：

```sh
riscdom-server --bind 127.0.0.1:7822 --data-dir target/demo-data-b --workspace target/demo-ws-b
riscdom --remote 127.0.0.1:7821 tasks dispatch --target <peer_node_id> --input "boot a hello"
```

**预期输出。** 近端节点把任务交给对端；近端节点给它的 `task_id` **随之旅行**，于是两个节点的审计
行能对齐。

**验收点。** 任务在第二个节点上运行，且 `riscdom tasks` / 两端的审计行命名**同一个** `task_id`。

**自动？** **否** —— 第二个节点是一个网络决定。脚本打印两条命令，并解释要核对的 `task_id`
连续性。

---

## 细节在哪里

- 能力到命令的映射：[capabilities.md](capabilities.md)。
- 每条命令背后的 API 端点：[control-plane-api.md](../control-plane-api.md) §5。
- 独立校验器的契约：`audit` crate 的 `audit-verify` 二进制。
- 一次发布要走的机走查：[manual-acceptance.md](../manual-acceptance.md) 与
  [golden-path-checklist.md](../golden-path-checklist.md)。
